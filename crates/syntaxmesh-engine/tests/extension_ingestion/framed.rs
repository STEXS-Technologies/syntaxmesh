use std::error::Error;

use syntaxmesh_core::{Edge, EdgeId, RelationKind};
use syntaxmesh_extension_ipc::{FrameCodec, FrameError, MAX_FRAME_BYTES, WIRE_VERSION};
use syntaxmesh_runtime_protocol::{OBSERVATION_SCHEMA_VERSION, Observation, ObservationOrigin};

use super::*;

fn graph_batch() -> Result<FactBatch, Box<dyn Error>> {
    let mut batch = fixture_batch();
    let source = batch.nodes.first().cloned().ok_or("fixture node missing")?;
    let mut target = source.clone();
    target.id = NodeId::derive(&[b"framed-database-node"]);
    target.name = "inventory-database".to_owned();
    batch.edges.push(Edge {
        id: EdgeId::derive(&[b"framed-service-database-edge"]),
        source: source.id,
        target: target.id,
        relation: RelationKind::External {
            namespace: NAMESPACE.to_owned(),
            relation: "uses".to_owned(),
        },
        provenance: source.provenance,
        extension_payload: None,
    });
    batch.nodes.push(target);
    Ok(batch)
}

fn decode(batch: &FactBatch) -> Result<FactBatch, Box<dyn Error>> {
    let codec = FrameCodec::new(MAX_FRAME_BYTES)?;
    let mut bytes = Vec::new();
    codec.write_batch(&mut bytes, batch)?;
    let mut input = bytes.as_slice();
    let decoded = codec.read_batch(&mut input)?.ok_or("missing frame")?;
    if !input.is_empty() || decoded != *batch {
        return Err("populated frame changed SDK facts or left trailing bytes".into());
    }
    Ok(decoded)
}

#[cfg(unix)]
#[test]
fn populated_batch_crosses_a_local_ipc_stream_before_engine_publication()
-> Result<(), Box<dyn Error>> {
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let expected = graph_batch()?;
    let producer_batch = expected.clone();
    let (mut producer, mut host) = UnixStream::pair()?;
    producer.set_write_timeout(Some(Duration::from_secs(10)))?;
    host.set_read_timeout(Some(Duration::from_secs(10)))?;
    let producer_thread = std::thread::spawn(move || -> Result<(), FrameError> {
        FrameCodec::new(MAX_FRAME_BYTES)?.write_batch(&mut producer, &producer_batch)
    });
    let codec = FrameCodec::new(MAX_FRAME_BYTES)?;
    let decoded = codec
        .read_batch_for(&mut host, &expected.manifest)?
        .ok_or("IPC producer closed without a batch")?;
    producer_thread
        .join()
        .map_err(|_panic| "IPC fixture producer panicked")??;
    if decoded != expected || codec.read_batch(&mut host)?.is_some() {
        return Err("local IPC changed facts or did not terminate at a clean boundary".into());
    }
    let generation = GenerationId::derive(&[b"local-ipc-publication"]);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        RepositoryId::derive(&[b"local-ipc-repository"]),
        WorktreeId::derive(&[b"local-ipc-worktree"]),
    );
    engine.ingest_extension_facts(decoded, IndexRunId::derive(&[b"local-ipc-run"]), generation)?;
    for node in expected.nodes {
        if engine.query(generation).node(node.id)?.as_ref() != Some(&node) {
            return Err("local IPC fact was not preserved by engine publication".into());
        }
    }
    Ok(())
}

#[test]
fn framed_facts_and_observations_match_embedded_ingestion_after_restart()
-> Result<(), Box<dyn Error>> {
    let snapshot = TestSnapshot::new();
    let repository = RepositoryId::derive(&[b"framed-equivalence"]);
    let worktree = WorktreeId::derive(&[b"framed-equivalence-worktree"]);
    let first = GenerationId::derive(&[b"framed-static"]);
    let second = GenerationId::derive(&[b"framed-runtime"]);
    let mut embedded = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    let mut framed = SyntaxMeshEngine::new(
        FileGraphStore::open(&snapshot.0)?,
        RustExtractor,
        repository,
        worktree,
    );
    let observation = Observation {
        schema_version: OBSERVATION_SCHEMA_VERSION,
        producer_namespace: NAMESPACE.to_owned(),
        producer_version: PRODUCER_VERSION.to_owned(),
        origin: ObservationOrigin::ClientProbe,
        observed_at_unix_nanos: 42,
        subject: "inventory".to_owned(),
        relation: "tier".to_owned(),
        object: "degraded".to_owned(),
        correlation_id: Some("external-request".to_owned()),
    };
    for (batch, generation, run) in [
        (
            graph_batch()?,
            first,
            IndexRunId::derive(&[b"framed-static-run"]),
        ),
        (
            runtime_batch(observation),
            second,
            IndexRunId::derive(&[b"framed-runtime-run"]),
        ),
    ] {
        embedded.ingest_extension_facts(batch.clone(), run, generation)?;
        framed.ingest_extension_facts(decode(&batch)?, run, generation)?;
        if framed.query(generation).graph_at(generation)?
            != embedded.query(generation).graph_at(generation)?
        {
            return Err("framed and embedded logical graph snapshots differ".into());
        }
    }
    let historical = embedded.query(second).graph_at(first)?;
    let current = embedded.query(second).graph_at(second)?;
    drop(framed);
    let reopened = SyntaxMeshEngine::new(
        FileGraphStore::open(&snapshot.0)?,
        RustExtractor,
        repository,
        worktree,
    );
    if reopened.query(second).graph_at(first)? != historical
        || reopened.query(second).graph_at(second)? != current
    {
        return Err("framed fact/observation history changed across restart".into());
    }
    let workflow = reopened.workflow_diagnostics()?;
    if workflow.completed_operations != 2 || workflow.prepared_operations != 0 {
        return Err("framed ingestion bypassed durable engine workflows".into());
    }
    Ok(())
}

#[test]
fn invalid_framed_batch_leaves_accepted_history_and_workflows_untouched()
-> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"framed-isolation"]);
    let worktree = WorktreeId::derive(&[b"framed-isolation-worktree"]);
    let generation = GenerationId::derive(&[b"framed-accepted"]);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    engine.ingest_extension_facts(
        decode(&fixture_batch())?,
        IndexRunId::derive(&[b"framed-accepted-run"]),
        generation,
    )?;
    let before = engine.query(generation).graph_at(generation)?;
    let mut invalid = fixture_batch();
    invalid.manifest.namespace = "fixture.impostor".to_owned();
    // Bypass the encoder deliberately, as an untrusted producer can.
    let payload = serde_json::to_vec(&invalid)?;
    let mut wire = b"SMEX".to_vec();
    wire.extend_from_slice(&WIRE_VERSION.to_be_bytes());
    wire.extend_from_slice(&u32::try_from(payload.len())?.to_be_bytes());
    wire.extend_from_slice(&payload);
    let decoded = FrameCodec::new(MAX_FRAME_BYTES)?.read_batch(&mut wire.as_slice());
    if !matches!(
        decoded,
        Err(FrameError::InvalidBatch(ExtensionError::NamespaceMismatch))
    ) {
        return Err("framed namespace mismatch was not rejected".into());
    }
    if engine.query(generation).graph_at(generation)? != before {
        return Err("rejected wire batch changed accepted history".into());
    }
    let diagnostics = engine.workflow_diagnostics()?;
    if diagnostics.completed_operations != 1
        || diagnostics.prepared_operations != 0
        || diagnostics.rejected_operations != 0
    {
        return Err("rejected wire batch created workflow records".into());
    }
    let next = GenerationId::derive(&[b"framed-after-rejection"]);
    engine.ingest_extension_facts(
        decode(&fixture_batch_for("fixture.metrics", "2"))?,
        IndexRunId::derive(&[b"framed-after-rejection-run"]),
        next,
    )?;
    if engine.query(next).graph_at(generation)? != before {
        return Err("subsequent producer changed the earlier snapshot".into());
    }
    Ok(())
}

#[test]
fn valid_but_unauthorized_producer_never_reaches_engine_publication() -> Result<(), Box<dyn Error>>
{
    let authorized = fixture_batch();
    let codec = FrameCodec::new(MAX_FRAME_BYTES)?;
    let incoming = fixture_batch_for("fixture.impersonator", "1.0.0");
    let mut wire = Vec::new();
    codec.write_batch(&mut wire, &incoming)?;
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        RepositoryId::derive(&[b"framed-auth"]),
        WorktreeId::derive(&[b"framed-auth-worktree"]),
    );
    let run = IndexRunId::derive(&[b"framed-auth-run"]);
    let generation = GenerationId::derive(&[b"framed-auth-generation"]);
    match codec.read_batch_for(&mut wire.as_slice(), &authorized.manifest) {
        Err(FrameError::UnauthorizedManifest) => {}
        Ok(Some(batch)) => {
            engine.ingest_extension_facts(batch, run, generation)?;
            return Err("unauthorized producer was exposed for publication".into());
        }
        other => return Err(format!("unexpected authorization result: {other:?}").into()),
    }
    if engine.status()?.is_some() || engine.workflow_diagnostics()?.completed_operations != 0 {
        return Err("unauthorized producer mutated engine state".into());
    }
    let mut valid_wire = Vec::new();
    codec.write_batch(&mut valid_wire, &authorized)?;
    let valid = codec
        .read_batch_for(&mut valid_wire.as_slice(), &authorized.manifest)?
        .ok_or("authorized frame missing")?;
    engine.ingest_extension_facts(valid, run, generation)?;
    if engine.workflow_diagnostics()?.completed_operations != 1 {
        return Err("valid producer could not publish after authorization rejection".into());
    }
    Ok(())
}
