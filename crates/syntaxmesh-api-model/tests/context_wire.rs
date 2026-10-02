//! Literal wire fixtures catch coordinated serializer/deserializer drift.

use std::error::Error;

use serde::{Serialize, de::DeserializeOwned};
use syntaxmesh_api_model::{
    CONTEXT_PACK_SCHEMA_VERSION, ContextItem, ContextItemKind, ContextPack, ContextRequest,
    ContextWarning, OmittedContextSummary,
};
use syntaxmesh_core::{
    EdgeId, EvidenceClass, GenerationId, NodeId, RepositoryId, StableId, WorktreeId,
};

fn golden<T: DeserializeOwned + Serialize + PartialEq>(
    fixture: &str,
    expected: &T,
) -> Result<(), Box<dyn Error>> {
    let decoded: T = serde_json::from_str(fixture)?;
    if &decoded != expected
        || serde_json::to_value(expected)? != serde_json::from_str::<serde_json::Value>(fixture)?
    {
        return Err("immutable context wire fixture differs from its typed contract".into());
    }
    Ok(())
}

fn pack(items: Vec<ContextItem>) -> ContextPack {
    ContextPack {
        schema_version: CONTEXT_PACK_SCHEMA_VERSION,
        query: "why Penelope".to_owned(),
        repository: RepositoryId(StableId([1; 32])),
        worktree: WorktreeId(StableId([2; 32])),
        generation: GenerationId(StableId([3; 32])),
        tokenizer: "fixture-tokenizer-v1".to_owned(),
        token_budget: 8192,
        token_count: 1024,
        items,
        warnings: Vec::new(),
        omitted: OmittedContextSummary::default(),
    }
}

fn item(kind: ContextItemKind, text: &str) -> ContextItem {
    ContextItem {
        rank: 1,
        evidence_class: None,
        kind,
        text: text.to_owned(),
        node_ids: vec![NodeId(StableId([4; 32]))],
        edge_ids: Vec::new(),
        source_path: None,
        line_start: None,
        line_end: None,
    }
}

#[test]
fn context_request_wire_is_immutable() -> Result<(), Box<dyn Error>> {
    golden(
        include_str!("fixtures/context-request-v1.json"),
        &ContextRequest {
            query: "why Penelope".to_owned(),
            seed_nodes: vec![NodeId(StableId([4; 32]))],
            token_budget: 8192,
            max_hops: 2,
            max_candidates: 64,
        },
    )
}

#[test]
fn classified_context_pack_wire_is_immutable() -> Result<(), Box<dyn Error>> {
    let mut source = item(
        ContextItemKind::SourceEvidence,
        "Use Penelope for durable workflows.",
    );
    source.evidence_class = Some(EvidenceClass::SourceFact);
    source.source_path = Some("docs/design.md".to_owned());
    source.line_start = Some(7);
    source.line_end = Some(8);
    let mut path = item(ContextItemKind::GraphPath, "SyntaxMesh --Uses--> Penelope");
    path.rank = 2;
    path.evidence_class = Some(EvidenceClass::SemanticInference);
    path.edge_ids.push(EdgeId(StableId([5; 32])));
    let mut expected = pack(vec![source, path]);
    expected.warnings.push(ContextWarning {
        code: "stale_source".to_owned(),
        message: "Changed source bytes excluded.".to_owned(),
        node_ids: Vec::new(),
    });
    expected.omitted = OmittedContextSummary {
        source_evidence: 1,
        signatures: 2,
        graph_paths: 3,
        summaries: 4,
    };
    golden(include_str!("fixtures/context-pack-v2.json"), &expected)
}

#[test]
fn legacy_pack_keeps_its_version_and_unknown_evidence_class() -> Result<(), Box<dyn Error>> {
    let fixture = include_str!("fixtures/context-pack-v1.json");
    let mut expected = pack(vec![item(
        ContextItemKind::Signature,
        "Penelope (External)",
    )]);
    expected.schema_version = 1;
    let decoded: ContextPack = serde_json::from_str(fixture)?;
    if decoded != expected {
        return Err("legacy pack was reclassified or its schema changed".into());
    }
    let restored: ContextPack = serde_json::from_slice(&serde_json::to_vec(&decoded)?)?;
    if restored != expected {
        return Err("legacy pack lost compatibility on reserialization".into());
    }
    Ok(())
}
