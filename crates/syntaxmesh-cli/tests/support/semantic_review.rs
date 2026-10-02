use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use syntaxmesh_core::{EvidenceClass, GenerationId, Node, NodeId, NodeKind, RelationKind};
use syntaxmesh_store::GraphStore;

type ReviewResult<T> = Result<T, Box<dyn Error>>;

fn invalid(message: &str) -> Box<dyn Error> {
    std::io::Error::other(message).into()
}

fn semantic_relation(relation: &RelationKind, expected: &str) -> bool {
    matches!(relation, RelationKind::External { namespace, relation }
        if namespace == "syntaxmesh.semantic" && relation == expected)
}

fn concept<'nodes>(
    nodes: &'nodes BTreeMap<NodeId, &Node>,
    targets: &[NodeId],
) -> ReviewResult<&'nodes Node> {
    let [target] = targets else {
        return Err(invalid("semantic review requires one exact role endpoint"));
    };
    let node = nodes
        .get(target)
        .copied()
        .ok_or_else(|| invalid("missing concept"))?;
    if !matches!(&node.kind, NodeKind::External { namespace, kind }
        if namespace == "syntaxmesh.semantic" && kind == "concept")
    {
        return Err(invalid("semantic role endpoint is not a concept"));
    }
    Ok(node)
}

pub(super) fn accepted_claims(
    store: &dyn GraphStore,
    generation: GenerationId,
    source_root: &Path,
) -> ReviewResult<Vec<Value>> {
    let snapshot = store.historical_snapshot(generation)?;
    let nodes = snapshot
        .nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<BTreeMap<_, _>>();
    let provenance = snapshot
        .provenance
        .iter()
        .map(|record| (record.id, record))
        .collect::<BTreeMap<_, _>>();
    let files = snapshot
        .files
        .iter()
        .map(|file| (file.file_id, file))
        .collect::<BTreeMap<_, _>>();
    let mut source_text = BTreeMap::new();
    let mut claims = Vec::new();
    for claim in snapshot.nodes.iter().filter(|node| matches!(&node.kind,
        NodeKind::External { namespace, kind } if namespace == "syntaxmesh.semantic" && kind == "claim"))
    {
        let role = |name| snapshot.edges.iter().filter(|edge| edge.source == claim.id && semantic_relation(&edge.relation, name)).map(|edge| edge.target).collect::<Vec<_>>();
        let subject = concept(&nodes, &role("subject"))?;
        let object = concept(&nodes, &role("object"))?;
        let relation = claim.name.strip_prefix(&format!("{} —", subject.name))
            .and_then(|name| name.strip_suffix(&format!("→ {}", object.name)))
            .filter(|name| !name.is_empty() && name.starts_with(|character: char| character.is_ascii_lowercase()) && name.chars().all(|character| character.is_ascii_lowercase() || character == '_'))
            .ok_or_else(|| invalid("semantic claim label disagrees with its role endpoints"))?;
        let mut evidence = Vec::new();
        for support in snapshot.edges.iter().filter(|edge| edge.target == claim.id && semantic_relation(&edge.relation, "supports")) {
            let chunk = nodes.get(&support.source).copied().ok_or_else(|| invalid("missing support chunk"))?;
            let record = provenance.get(&support.provenance).copied().ok_or_else(|| invalid("missing support provenance"))?;
            let location = record.source.as_ref().ok_or_else(|| invalid("support lacks a source location"))?;
            let chunk_location = chunk.source.as_ref().ok_or_else(|| invalid("support chunk lacks a source location"))?;
            if chunk.kind != NodeKind::DocumentChunk || record.producer_namespace != "syntaxmesh.semantic"
                || record.evidence_class != EvidenceClass::SemanticInference
                || location.file_id != chunk_location.file_id || location.content_hash != chunk_location.content_hash
                || location.span.start_byte < chunk_location.span.start_byte || location.span.end_byte > chunk_location.span.end_byte {
                return Err(invalid("semantic support provenance disagrees with its source chunk"));
            }
            let file = files.get(&location.file_id).copied().ok_or_else(|| invalid("support source file is missing"))?;
            if file.content_hash != location.content_hash {
                return Err(invalid("support source hash disagrees with accepted file"));
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = source_text.entry(file.file_id) {
                let bytes = fs::read(source_root.join(&file.normalized_path))?;
                if blake3::hash(&bytes).as_bytes() != &file.content_hash {
                    return Err(invalid("retained source disagrees with accepted file hash"));
                }
                entry.insert(String::from_utf8(bytes)?);
            }
            let text = source_text.get(&file.file_id).ok_or_else(|| invalid("source text missing"))?;
            let start = usize::try_from(location.span.start_byte)?;
            let end = usize::try_from(location.span.end_byte)?;
            let quote = text.get(start..end).filter(|quote| !quote.is_empty()).ok_or_else(|| invalid("support span is not a nonempty UTF-8 range"))?;
            evidence.push(json!({
                "support_edge_id":support.id, "chunk_id":chunk.id,
                "provenance_id":record.id, "evidence_class":record.evidence_class,
                "producer_version":record.producer_version,
                "source_path":file.normalized_path, "content_hash":hex::encode(file.content_hash),
                "start_byte":start, "end_byte":end, "quote":quote
            }));
        }
        if evidence.is_empty() {
            return Err(invalid("accepted semantic claim has no supporting source evidence"));
        }
        claims.push(json!({"claim_id":claim.id, "label":claim.name,
            "subject_id":subject.id, "subject":subject.name, "relation":relation,
            "object_id":object.id, "object":object.name, "evidence":evidence}));
    }
    Ok(claims)
}
