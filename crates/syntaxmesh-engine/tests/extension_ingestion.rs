use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use syntaxmesh_api_model::GraphRecord;
use syntaxmesh_core::{
    EvidenceClass, ExtensionPayload, GenerationId, IndexRunId, Node, NodeId, NodeKind, Provenance,
    ProvenanceId, RepositoryId, WorktreeId,
};
use syntaxmesh_engine::{EngineError, SyntaxMeshEngine};
use syntaxmesh_extension_sdk::{
    Capability, EXTENSION_MANIFEST_SCHEMA_VERSION, ExtensionError, ExtensionManifest, FactBatch,
};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::{FileGraphStore, InMemoryGraphStore};

#[path = "extension_ingestion/framed.rs"]
mod framed;

const NAMESPACE: &str = "fixture.catalog";
const PRODUCER_VERSION: &str = "1.0.0";
static NEXT_SNAPSHOT: AtomicU64 = AtomicU64::new(0);

struct TestSnapshot(PathBuf);

impl TestSnapshot {
    fn new() -> Self {
        let sequence = NEXT_SNAPSHOT.fetch_add(1, Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!(
            "syntaxmesh-observation-fixture-{}-{sequence}.bin",
            std::process::id()
        )))
    }
}

impl Drop for TestSnapshot {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.0));
    }
}

fn fixture_batch() -> FactBatch {
    fixture_batch_for(NAMESPACE, PRODUCER_VERSION)
}

fn fixture_batch_for(namespace: &str, producer_version: &str) -> FactBatch {
    let provenance = Provenance {
        id: ProvenanceId::derive(&[namespace.as_bytes(), b"fixture"]),
        producer_namespace: namespace.to_owned(),
        producer_version: producer_version.to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let node = Node {
        id: NodeId::derive(&[namespace.as_bytes(), b"service", b"inventory"]),
        kind: NodeKind::External {
            namespace: namespace.to_owned(),
            kind: "service".to_owned(),
        },
        name: "inventory".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: Some(ExtensionPayload {
            namespace: namespace.to_owned(),
            schema_version: 1,
            bytes: br#"{"tier":"critical"}"#.to_vec(),
        }),
    };
    FactBatch {
        manifest: ExtensionManifest {
            schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
            namespace: namespace.to_owned(),
            producer_version: producer_version.to_owned(),
            capabilities: vec![Capability::AnalysisFacts],
        },
        provenance: vec![provenance],
        nodes: vec![node],
        edges: Vec::new(),
        observations: Vec::new(),
    }
}

#[test]
fn invalid_extension_batch_does_not_mutate_state_or_block_another_extension() {
    let result = isolates_invalid_extension_batch();
    assert!(result.is_ok(), "extension isolation failed: {result:?}");
}

fn isolates_invalid_extension_batch() -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"extension-isolation-repository"]);
    let worktree = WorktreeId::derive(&[b"extension-isolation-worktree"]);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );

    let mut malformed = fixture_batch_for("fixture.bad", "1.0.0");
    let payload = malformed
        .nodes
        .first_mut()
        .and_then(|node| node.extension_payload.as_mut())
        .ok_or_else(|| std::io::Error::other("fixture extension payload missing"))?;
    payload.namespace = "fixture.other".to_owned();
    let rejected = engine.ingest_extension_facts(
        malformed,
        IndexRunId::derive(&[b"extension-isolation-rejected-run"]),
        GenerationId::derive(&[b"extension-isolation-rejected-generation"]),
    );
    if !matches!(
        &rejected,
        Err(EngineError::Extension(ExtensionError::NamespaceMismatch))
    ) {
        return Err(std::io::Error::other(format!(
            "malformed extension batch was not rejected at its namespace boundary: {rejected:?}"
        ))
        .into());
    }
    let diagnostic = rejected
        .as_ref()
        .err()
        .map(ToString::to_string)
        .ok_or_else(|| std::io::Error::other("rejected extension error was not retained"))?;
    if !diagnostic.contains("extension validation failed")
        || !diagnostic.contains("namespace does not match")
    {
        return Err(std::io::Error::other(format!(
            "extension rejection did not provide an actionable diagnostic: {diagnostic}"
        ))
        .into());
    }
    if engine.status()?.is_some() {
        return Err(
            std::io::Error::other("rejected extension batch mutated the canonical graph").into(),
        );
    }
    let after_rejection = engine.workflow_diagnostics()?;
    if after_rejection.prepared_operations != 0
        || after_rejection.completed_operations != 0
        || after_rejection.rejected_operations != 0
    {
        return Err(std::io::Error::other(
            "pre-publication extension validation created workflow records",
        )
        .into());
    }

    let valid_namespace = "fixture.metrics";
    let valid_generation = GenerationId::derive(&[b"extension-isolation-valid-generation"]);
    let valid_node_id = NodeId::derive(&[valid_namespace.as_bytes(), b"service", b"inventory"]);
    engine.ingest_extension_facts(
        fixture_batch_for(valid_namespace, "2.0.0"),
        IndexRunId::derive(&[b"extension-isolation-valid-run"]),
        valid_generation,
    )?;
    if engine
        .query(valid_generation)
        .node(valid_node_id)?
        .is_none()
    {
        return Err(std::io::Error::other(
            "valid extension could not publish after another extension failed validation",
        )
        .into());
    }
    Ok(())
}

fn runtime_batch(observation: syntaxmesh_runtime_protocol::Observation) -> FactBatch {
    FactBatch {
        manifest: ExtensionManifest {
            schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
            namespace: NAMESPACE.to_owned(),
            producer_version: PRODUCER_VERSION.to_owned(),
            capabilities: vec![Capability::RuntimeObservations],
        },
        provenance: Vec::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
        observations: vec![observation],
    }
}

#[test]
fn public_extension_batch_publishes_namespaced_facts_through_engine_workflow() {
    let result = ingest_public_extension_batch();
    assert!(result.is_ok(), "extension ingestion failed: {result:?}");
}

fn ingest_public_extension_batch() -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"extension-fixture-repository"]);
    let worktree = WorktreeId::derive(&[b"extension-fixture-worktree"]);
    let node_id = NodeId::derive(&[NAMESPACE.as_bytes(), b"service", b"inventory"]);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    let receipt = engine.ingest_extension_facts(
        fixture_batch(),
        IndexRunId::derive(&[b"extension-fixture-run"]),
        GenerationId::derive(&[b"extension-fixture-generation"]),
    )?;
    if receipt.verification != syntaxmesh_workflow::WorkflowStatus::Durable {
        return Err(std::io::Error::other("extension generation was not durably published").into());
    }
    let workflow = engine.workflow_diagnostics()?;
    if workflow.completed_operations != 1 || workflow.prepared_operations != 0 {
        return Err(std::io::Error::other("engine workflow diagnostics are inconsistent").into());
    }
    let node = engine
        .query(receipt.publication.generation.generation)
        .node(node_id)?
        .ok_or_else(|| std::io::Error::other("published extension node was not queryable"))?;
    if !matches!(
        node.kind,
        NodeKind::External { ref namespace, ref kind }
            if namespace == NAMESPACE && kind == "service"
    ) || node
        .extension_payload
        .as_ref()
        .is_none_or(|payload| payload.namespace != NAMESPACE || payload.schema_version != 1)
    {
        return Err(std::io::Error::other("published fact lost its extension namespace").into());
    }
    Ok(())
}

#[test]
fn public_runtime_observation_is_durable_separate_and_exportable() {
    let result = persist_runtime_observation();
    assert!(
        result.is_ok(),
        "runtime observation test failed: {result:?}"
    );
}

fn persist_runtime_observation() -> Result<(), Box<dyn Error>> {
    let snapshot = TestSnapshot::new();
    let repository = RepositoryId::derive(&[b"observation-fixture-repository"]);
    let worktree = WorktreeId::derive(&[b"observation-fixture-worktree"]);
    let static_generation = GenerationId::derive(&[b"observation-fixture-static-generation"]);
    let runtime_generation = GenerationId::derive(&[b"observation-fixture-runtime-generation"]);
    let observation = syntaxmesh_runtime_protocol::Observation {
        schema_version: syntaxmesh_runtime_protocol::OBSERVATION_SCHEMA_VERSION,
        producer_namespace: NAMESPACE.to_owned(),
        producer_version: PRODUCER_VERSION.to_owned(),
        origin: syntaxmesh_runtime_protocol::ObservationOrigin::ServerProbe,
        observed_at_unix_nanos: 1,
        subject: "inventory".to_owned(),
        relation: "tier".to_owned(),
        object: "degraded".to_owned(),
        correlation_id: Some("request-42".to_owned()),
    };
    let store = FileGraphStore::open(&snapshot.0)?;
    let mut engine = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);
    engine.ingest_extension_facts(
        fixture_batch(),
        IndexRunId::derive(&[b"observation-fixture-static-run"]),
        static_generation,
    )?;
    let static_records = engine.query(static_generation).export_records()?;
    let static_nodes_before = static_records
        .iter()
        .filter_map(|record| match record {
            GraphRecord::Node { node, .. } if node.kind != NodeKind::RuntimeObservation => {
                Some(node.clone())
            }
            GraphRecord::Header { .. }
            | GraphRecord::Node { .. }
            | GraphRecord::Edge { .. }
            | GraphRecord::Provenance { .. }
            | GraphRecord::Footer { .. } => None,
        })
        .collect::<Vec<_>>();
    let receipt = engine.ingest_extension_facts(
        runtime_batch(observation.clone()),
        IndexRunId::derive(&[b"observation-fixture-run"]),
        runtime_generation,
    )?;
    if receipt.publication.generation.generation != runtime_generation {
        return Err(std::io::Error::other("engine published an unexpected generation").into());
    }
    drop(engine);

    let reopened_store = FileGraphStore::open(&snapshot.0)?;
    let reopened_engine =
        SyntaxMeshEngine::new(reopened_store, RustExtractor, repository, worktree);
    let records = reopened_engine.query(runtime_generation).export_records()?;
    let static_nodes_after = records
        .iter()
        .filter_map(|record| match record {
            GraphRecord::Node { node, .. } if node.kind != NodeKind::RuntimeObservation => {
                Some(node.clone())
            }
            GraphRecord::Header { .. }
            | GraphRecord::Node { .. }
            | GraphRecord::Edge { .. }
            | GraphRecord::Provenance { .. }
            | GraphRecord::Footer { .. } => None,
        })
        .collect::<Vec<_>>();
    if static_nodes_before != static_nodes_after {
        return Err(std::io::Error::other(
            "runtime observation changed previously published static facts",
        )
        .into());
    }
    let observation_node = records.iter().find_map(|record| match record {
        GraphRecord::Node { node, .. } if node.kind == NodeKind::RuntimeObservation => Some(node),
        GraphRecord::Header { .. }
        | GraphRecord::Node { .. }
        | GraphRecord::Edge { .. }
        | GraphRecord::Provenance { .. }
        | GraphRecord::Footer { .. } => None,
    });
    let observation_node = observation_node
        .ok_or_else(|| std::io::Error::other("runtime observation missing after restart"))?;
    let payload = observation_node
        .extension_payload
        .as_ref()
        .ok_or_else(|| std::io::Error::other("runtime observation payload missing"))?;
    let decoded: syntaxmesh_runtime_protocol::Observation = serde_json::from_slice(&payload.bytes)?;
    if decoded != observation
        || payload.namespace != NAMESPACE
        || payload.schema_version != observation.schema_version
    {
        return Err(std::io::Error::other("observation payload changed during persistence").into());
    }
    let observation_provenance = records.iter().any(|record| match record {
        GraphRecord::Provenance { provenance, .. } => {
            provenance.id == observation_node.provenance
                && provenance.evidence_class == EvidenceClass::RuntimeObserved
                && provenance.producer_namespace == NAMESPACE
        }
        GraphRecord::Header { .. }
        | GraphRecord::Node { .. }
        | GraphRecord::Edge { .. }
        | GraphRecord::Footer { .. } => false,
    });
    if !observation_provenance {
        return Err(std::io::Error::other(
            "runtime observation lost its runtime-observed provenance",
        )
        .into());
    }
    let observation_history = reopened_engine
        .query(runtime_generation)
        .fact_history(syntaxmesh_core::FactRef::Node(observation_node.id))?;
    if !matches!(
        observation_history.as_slice(),
        [syntaxmesh_api_model::TemporalRecord::FactVersion {
            query_mode: syntaxmesh_api_model::TemporalQueryMode::HistoricalConclusion,
            observed_at: Some(syntaxmesh_core::ObservationTime(1)),
            accepted_at: Some(_),
            payload: syntaxmesh_core::FactPayload::Node(history_node),
            ..
        }] if history_node.id == observation_node.id
    ) {
        return Err(std::io::Error::other(
            "runtime fact history did not preserve distinct observation and acceptance times",
        )
        .into());
    }
    let observed_timeline = reopened_engine.query(runtime_generation).observed_between(
        syntaxmesh_core::ObservationTime(0),
        syntaxmesh_core::ObservationTime(2),
        None,
        5,
    )?;
    if !matches!(
        observed_timeline.as_slice(),
        [
            syntaxmesh_api_model::TemporalRecord::ObservedFact {
                query_mode: syntaxmesh_api_model::TemporalQueryMode::ObservationTimeline,
                fact: syntaxmesh_core::FactRef::Node(id),
                observed_at: syntaxmesh_core::ObservationTime(1),
                accepted_at: Some(_),
                ..
            },
            syntaxmesh_api_model::TemporalRecord::ObservedTimelineFooter {
                query_mode: syntaxmesh_api_model::TemporalQueryMode::ObservationTimeline,
                returned: 1,
                has_more: false,
                next_cursor: None,
                ..
            }
        ] if *id == observation_node.id
    ) {
        return Err(std::io::Error::other(
            "observed-time range query did not return the indexed runtime fact",
        )
        .into());
    }
    if records
        .iter()
        .any(|record| matches!(record, GraphRecord::Edge { .. }))
    {
        return Err(std::io::Error::other(
            "opaque runtime identifiers were incorrectly converted to graph edges",
        )
        .into());
    }
    Ok(())
}
