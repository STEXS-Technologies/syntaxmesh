use std::error::Error;

use syntaxmesh_core::{
    EvidenceClass, FileId, FileVersion, GenerationId, GraphDelta, IndexRunId, Node, NodeId,
    NodeKind, Provenance, ProvenanceId, RepositoryId, SourceLocation, SourceSpan, WorktreeId,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_semantic::{
    SemanticClaim, SemanticDocumentChunk, SemanticEvidence, SemanticOutput,
    SemanticProviderIdentity, SemanticRequest,
};
use syntaxmesh_store::{GraphStore, InMemoryGraphStore};

#[test]
fn replacement_and_clearing_ignore_inactive_retained_provenance() -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"semantic-replacement"]);
    let worktree = WorktreeId::derive(&[b"semantic-replacement"]);
    let base = GenerationId::derive(&[b"semantic-replacement-base"]);
    let first = GenerationId::derive(&[b"semantic-replacement-first"]);
    let revised = GenerationId::derive(&[b"semantic-replacement-revised"]);
    let cleared = GenerationId::derive(&[b"semantic-replacement-cleared"]);
    let unused = GenerationId::derive(&[b"semantic-replacement-noop"]);
    let content = "SyntaxMesh uses Penelope for durable indexing.";
    let file_id = FileId::derive(&[b"docs/architecture.md"]);
    let content_hash = *blake3::hash(content.as_bytes()).as_bytes();
    let location = SourceLocation {
        file_id,
        content_hash,
        span: SourceSpan::new(0, u64::try_from(content.len())?)?,
    };
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"semantic-replacement-source"]),
        producer_namespace: "syntaxmesh.lang.documentation".to_owned(),
        producer_version: "fixture".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: Some(location.clone()),
    };
    let chunk = Node {
        id: NodeId::derive(&[b"semantic-replacement-chunk"]),
        kind: NodeKind::DocumentChunk,
        name: content.to_owned(),
        owner_file: Some(file_id),
        source: Some(location),
        provenance: provenance.id,
        extension_payload: None,
    };
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"semantic-replacement-source"]),
        expected_base: None,
        next_generation: base,
        changed_files: vec![FileVersion {
            file_id,
            normalized_path: "docs/architecture.md".to_owned(),
            content_hash,
            size_bytes: u64::try_from(content.len())?,
        }],
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![chunk.clone()],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let mut identity = SemanticProviderIdentity {
        provider: "fixture".to_owned(),
        model: "fixture".to_owned(),
        model_revision: "first".to_owned(),
        prompt_version: "fixture".to_owned(),
        prompt_hash: [1; 32],
        configuration_hash: [2; 32],
    };
    let chunks = vec![SemanticDocumentChunk {
        node: chunk.clone(),
        text: content.to_owned(),
        context: Vec::new(),
    }];
    let output = SemanticOutput {
        claims: vec![SemanticClaim {
            subject: "SyntaxMesh".to_owned(),
            relation: "uses".to_owned(),
            object: "Penelope".to_owned(),
            evidence: vec![SemanticEvidence {
                chunk_content_hash: content_hash,
                quote: content.to_owned(),
            }],
        }],
    };
    let first_batch =
        SemanticRequest::new(identity.clone(), chunks.clone())?.into_fact_batch(output.clone())?;
    let mut engine = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);
    let run = |label: &[u8]| IndexRunId::derive(&[b"semantic-replacement", label]);
    if engine
        .replace_semantic_facts(first_batch.clone(), run(b"first"), first)?
        .is_none()
        || engine
            .replace_semantic_facts(first_batch.clone(), run(b"repeat-first"), unused)?
            .is_some()
    {
        return Err("first semantic replacement is not idempotent".into());
    }
    identity.model_revision = "revised".to_owned();
    let revised_batch = SemanticRequest::new(identity.clone(), chunks)?.into_fact_batch(output)?;
    if engine
        .replace_semantic_facts(revised_batch.clone(), run(b"revised"), revised)?
        .is_none()
        || engine
            .replace_semantic_facts(revised_batch.clone(), run(b"repeat-revised"), unused)?
            .is_some()
    {
        return Err("revised semantic replacement is not idempotent".into());
    }
    let empty = SemanticOutput::empty_fact_batch(&identity);
    if engine
        .replace_semantic_facts(empty.clone(), run(b"clear"), cleared)?
        .is_none()
        || engine
            .replace_semantic_facts(empty.clone(), run(b"repeat-clear"), unused)?
            .is_some()
        || engine
            .replace_semantic_facts(empty, run(b"repeat-clear-again"), unused)?
            .is_some()
    {
        return Err("semantic clearing is not idempotent".into());
    }
    if engine
        .current_generation(repository, worktree)?
        .map(|manifest| manifest.generation)
        != Some(cleared)
        || engine.query(cleared).node(chunk.id)? != Some(chunk)
    {
        return Err("semantic clearing changed deterministic source facts".into());
    }
    let historical = engine.query(cleared).graph_at(first)?;
    for node in &first_batch.nodes {
        if historical
            .nodes
            .iter()
            .find(|candidate| candidate.id == node.id)
            != Some(node)
            || engine.query(cleared).node(node.id)?.is_some()
        {
            return Err("semantic clearing damaged historical or current facts".into());
        }
    }
    let snapshot = engine.query(cleared).graph_at(cleared)?;
    for record in first_batch
        .provenance
        .iter()
        .chain(&revised_batch.provenance)
    {
        if !snapshot.provenance.contains(record) {
            return Err("semantic clearing pruned retained provenance".into());
        }
    }
    if engine.query(cleared).graph_at(unused).is_ok() {
        return Err("idempotent replacement accepted an unused generation".into());
    }
    Ok(())
}
