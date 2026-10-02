//! Exercise multi-page temporal context against real adapters, not a mock port.

use super::{
    ContextFixtureCounter, ContextFixtureSource, ContextItemKind, ContextPack, ContextRequest,
    Edge, EdgeId, EvidenceClass, FileGraphStore, GenerationId, GraphDelta, GraphStore,
    InMemoryGraphStore, IndexRunId, NodeId, ProvenanceId, Query, RelationKind, SqliteGraphStore,
    StoreError, open_migrated, open_turso, publish_context_fixture, repository, worktree,
};
use syntaxmesh_core::EdgeDirection;

fn dense_delta(base: GenerationId) -> GraphDelta {
    let caller = NodeId::derive(&[b"context-conformance-caller"]);
    let helper = NodeId::derive(&[b"context-conformance-helper"]);
    let provenance = ProvenanceId::derive(&[b"context-conformance-provenance"]);
    let mut edges = Vec::new();
    for index in 0_u32..1001 {
        for (tag, source, target) in [
            (b"dense-outgoing".as_slice(), caller, helper),
            (b"dense-incoming".as_slice(), helper, caller),
        ] {
            edges.push(Edge {
                id: EdgeId::derive(&[tag, &index.to_le_bytes()]),
                source,
                target,
                relation: RelationKind::Calls,
                provenance,
                extension_payload: None,
            });
        }
    }
    edges.push(Edge {
        id: EdgeId::derive(&[b"dense-self-edge"]),
        source: caller,
        target: caller,
        relation: RelationKind::Calls,
        provenance,
        extension_payload: None,
    });
    GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"dense-context-run"]),
        expected_base: Some(base),
        next_generation: GenerationId::derive(&[b"dense-context-generation"]),
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: edges,
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    }
}

fn pack(
    store: &dyn GraphStore,
    generation: GenerationId,
    capacity: u16,
    historical: bool,
) -> Result<ContextPack, StoreError> {
    let started = std::env::var_os("SYNTAXMESH_STORE_PROFILE").map(|_| std::time::Instant::now());
    let query = Query::new(store, generation);
    let request = ContextRequest {
        query: "caller".to_owned(),
        seed_nodes: vec![NodeId::derive(&[b"context-conformance-caller"])],
        token_budget: 4096,
        max_hops: 1,
        max_candidates: capacity,
    };
    let source =
        ContextFixtureSource(b"pub fn caller() { helper(); }\npub fn helper() {}\n".to_vec());
    let result = if historical {
        query.historical_context(&request, &source, &ContextFixtureCounter)
    } else {
        query.context(&request, &source, &ContextFixtureCounter)
    };
    if let Some(started) = started {
        eprintln!(
            "context_fixture_stage historical={historical} capacity={capacity} elapsed_us={}",
            started.elapsed().as_micros()
        );
    }
    result.map_err(|error| StoreError::Backend(error.to_string()))
}

fn compare(
    store: &dyn GraphStore,
    generation: GenerationId,
    expected: &[ContextPack; 2],
    historical: bool,
) -> Result<(), StoreError> {
    for (capacity, reference) in [1, 2].into_iter().zip(expected) {
        let actual = pack(store, generation, capacity, historical)?;
        let encoded = serde_json::to_string(&actual)
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        let byte_count =
            u64::try_from(encoded.len()).map_err(|error| StoreError::Backend(error.to_string()))?;
        if &actual != reference
            || actual.token_count != byte_count
            || actual.token_count > actual.token_budget
            || actual
                .items
                .iter()
                .any(|item| item.evidence_class != Some(EvidenceClass::SourceFact))
            || !actual
                .items
                .iter()
                .any(|item| item.kind == ContextItemKind::SourceEvidence)
        {
            return Err(StoreError::Integrity(
                "multi-page context parity or evidence differs".to_owned(),
            ));
        }
    }
    Ok(())
}

fn verify_page_boundaries(
    store: &dyn GraphStore,
    generation: GenerationId,
) -> Result<(), StoreError> {
    let caller = NodeId::derive(&[b"context-conformance-caller"]);
    for (direction, remainder) in [(EdgeDirection::Outgoing, 3), (EdgeDirection::Incoming, 2)] {
        let first = store.historical_incident_edges(generation, caller, direction, None, 1000)?;
        let last = first
            .items
            .last()
            .ok_or_else(|| StoreError::Integrity("empty dense edge page".to_owned()))?
            .id;
        let second =
            store.historical_incident_edges(generation, caller, direction, Some(last), 1000)?;
        if first.items.len() != 1000
            || !first.has_more
            || second.items.len() != remainder
            || second.has_more
            || second.items.iter().any(|edge| edge.id <= last)
        {
            return Err(StoreError::Integrity(
                "dense fixture did not cross incidence page boundaries".to_owned(),
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "benchmark-instrumentation")]
fn verify_turso_hydration_read_counts(
    store: &dyn GraphStore,
    generation: GenerationId,
) -> Result<(), StoreError> {
    let caller = NodeId::derive(&[b"context-conformance-caller"]);
    for direction in [EdgeDirection::Outgoing, EdgeDirection::Incoming] {
        let first = store.historical_incident_edges(generation, caller, direction, None, 1000)?;
        let last = first
            .items
            .last()
            .ok_or_else(|| StoreError::Integrity("empty hydration-count fixture".to_owned()))?
            .id;
        let tail =
            store.historical_incident_edges(generation, caller, direction, Some(last), 1000)?;
        for page in [first, tail] {
            let metrics = page.read_metrics;
            let expected_reads = 2_usize
                .saturating_add(metrics.incidence_index_pages_loaded)
                .saturating_add(page.items.len());
            if metrics.sql_read_statements != expected_reads
                || metrics.edge_payload_rows_fetched != page.items.len()
            {
                return Err(StoreError::Integrity(
                    "historical hydration point-read counts differ".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn multi_page_context_matches_current_and_retains_old_edges_across_restart()
-> Result<(), StoreError> {
    let directory = tempfile::tempdir().map_err(|error| StoreError::Backend(error.to_string()))?;
    let file_path = directory.path().join("context.snapshot");
    let sqlite_path = directory.path().join("context.sqlite");
    let turso_path = directory.path().join("context.turso");
    let mut memory = InMemoryGraphStore::new();
    let mut file = FileGraphStore::open(&file_path)?;
    let mut sqlite = open_migrated(&sqlite_path)?;
    let mut turso = open_turso(&turso_path)?;
    let base = publish_context_fixture(&mut memory)?;
    let dense = dense_delta(base);
    let generation = dense.next_generation;
    memory.apply_delta(dense.clone())?;
    let expected = [
        pack(&memory, generation, 1, false)?,
        pack(&memory, generation, 2, false)?,
    ];
    for store in [&mut file as &mut dyn GraphStore, &mut sqlite, &mut turso] {
        publish_context_fixture(store)?;
        store.apply_delta(dense.clone())?;
    }
    for store in [&memory as &dyn GraphStore, &file, &sqlite, &turso] {
        verify_page_boundaries(store, generation)?;
        compare(store, generation, &expected, false)?;
        compare(store, generation, &expected, true)?;
    }
    #[cfg(feature = "benchmark-instrumentation")]
    verify_turso_hydration_read_counts(&turso, generation)?;
    let mut reduced = dense;
    reduced.expected_base = Some(generation);
    reduced.next_generation = GenerationId::derive(&[b"reduced-context-generation"]);
    reduced.run_id = IndexRunId::derive(&[b"reduced-context-run"]);
    reduced.remove_edges = reduced.upsert_edges.iter().map(|edge| edge.id).collect();
    reduced.upsert_edges.clear();
    let latest = reduced.next_generation;
    for store in [
        &mut memory as &mut dyn GraphStore,
        &mut file,
        &mut sqlite,
        &mut turso,
    ] {
        store.apply_delta(reduced.clone())?;
        verify_page_boundaries(store, generation)?;
        compare(store, generation, &expected, true)?;
    }
    let current = [
        pack(&memory, latest, 1, false)?,
        pack(&memory, latest, 2, false)?,
    ];
    drop(file);
    drop(sqlite);
    drop(turso);
    for store in [
        &FileGraphStore::open(&file_path)? as &dyn GraphStore,
        &SqliteGraphStore::open(&sqlite_path)?,
        &open_turso(&turso_path)?,
    ] {
        verify_page_boundaries(store, generation)?;
        compare(store, generation, &expected, true)?;
        compare(store, latest, &current, false)?;
        compare(store, latest, &current, true)?;
    }
    Ok(())
}
