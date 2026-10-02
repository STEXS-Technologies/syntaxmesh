use syntaxmesh_core::{
    EvidenceClass, ExtensionPayload, Node, NodeId, NodeKind, Provenance, ProvenanceId,
};
#[cfg(feature = "embedded")]
use syntaxmesh_core::{GenerationId, IndexRunId, RepositoryId, WorktreeId};
#[cfg(feature = "embedded")]
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_extension_sdk::{
    Capability, EXTENSION_MANIFEST_SCHEMA_VERSION, ExtensionManifest, FactBatch,
};
#[cfg(feature = "embedded")]
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_runtime_protocol::{OBSERVATION_SCHEMA_VERSION, Observation, ObservationOrigin};
#[cfg(feature = "embedded")]
use syntaxmesh_store::InMemoryGraphStore;

const NAMESPACE: &str = "fixture.catalog";
const VERSION: &str = "1.0.0";

pub(super) fn emit_frame(truncated: bool) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use syntaxmesh_extension_ipc::{FrameCodec, MAX_FRAME_BYTES};
    let node_id = NodeId::derive(&[NAMESPACE.as_bytes(), b"service", b"inventory"]);
    let mut frame = Vec::new();
    FrameCodec::new(MAX_FRAME_BYTES)?.write_batch(&mut frame, &catalog_facts(node_id))?;
    let mut stdout = std::io::stdout().lock();
    if truncated {
        stdout.write_all(frame.get(..11).ok_or("fixture header missing")?)?;
        stdout.flush()?;
        return Err("injected external producer failure after partial header".into());
    }
    stdout.write_all(&frame)?;
    stdout.flush()?;
    Ok(())
}

#[cfg(feature = "embedded")]
pub(super) fn publish_and_query() -> Result<(), Box<dyn std::error::Error>> {
    let repository = RepositoryId::derive(&[b"out-of-tree-repository"]);
    let worktree = WorktreeId::derive(&[b"out-of-tree-worktree"]);
    let generation = GenerationId::derive(&[b"out-of-tree-generation"]);
    let node_id = NodeId::derive(&[NAMESPACE.as_bytes(), b"service", b"inventory"]);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    let receipt = engine.ingest_extension_facts(
        catalog_facts(node_id),
        IndexRunId::derive(&[b"out-of-tree-run"]),
        generation,
    )?;
    let node = engine
        .query(receipt.publication.generation.generation)
        .node(node_id)?
        .ok_or("published extension fact was not queryable")?;
    if !matches!(&node.kind, NodeKind::External { namespace, kind }
        if namespace == NAMESPACE && kind == "service")
        || node
            .extension_payload
            .as_ref()
            .is_none_or(|payload| payload.namespace != NAMESPACE || payload.schema_version != 1)
    {
        return Err("published fact lost its public extension identity".into());
    }
    let observations = engine
        .query(receipt.publication.generation.generation)
        .search("out-of-tree-probe reports healthy", 8)?;
    if !observations
        .iter()
        .any(|candidate| candidate.kind == NodeKind::RuntimeObservation)
    {
        return Err("published runtime observation was not queryable".into());
    }
    println!(
        "out-of-tree extension published and queried {NAMESPACE}:{}",
        node.name
    );
    Ok(())
}

fn catalog_facts(node_id: NodeId) -> FactBatch {
    let provenance_id = ProvenanceId::derive(&[NAMESPACE.as_bytes(), b"inventory"]);
    FactBatch {
        manifest: ExtensionManifest {
            schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
            namespace: NAMESPACE.to_owned(),
            producer_version: VERSION.to_owned(),
            capabilities: vec![Capability::AnalysisFacts, Capability::RuntimeObservations],
        },
        provenance: vec![Provenance {
            id: provenance_id,
            producer_namespace: NAMESPACE.to_owned(),
            producer_version: VERSION.to_owned(),
            evidence_class: EvidenceClass::UserAsserted,
            source: None,
        }],
        nodes: vec![Node {
            id: node_id,
            kind: NodeKind::External {
                namespace: NAMESPACE.to_owned(),
                kind: "service".to_owned(),
            },
            name: "inventory".to_owned(),
            owner_file: None,
            source: None,
            provenance: provenance_id,
            extension_payload: Some(ExtensionPayload {
                namespace: NAMESPACE.to_owned(),
                schema_version: 1,
                bytes: br#"{"tier":"critical"}"#.to_vec(),
            }),
        }],
        edges: Vec::new(),
        observations: vec![Observation {
            schema_version: OBSERVATION_SCHEMA_VERSION,
            producer_namespace: NAMESPACE.to_owned(),
            producer_version: VERSION.to_owned(),
            origin: ObservationOrigin::ServerProbe,
            observed_at_unix_nanos: 7,
            subject: "out-of-tree-probe".to_owned(),
            relation: "reports".to_owned(),
            object: "healthy".to_owned(),
            correlation_id: Some("fixture-observation".to_owned()),
        }],
    }
}
