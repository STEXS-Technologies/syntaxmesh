use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use syntaxmesh_core::{
    AcceptanceTime, ConsequenceDelta, ConsequenceDerivation, ConsequenceEdge, ConsequenceEdgeId,
    ConsequenceKind, Edge, EdgeDirection, EdgeId, EvidenceClass, ExtensionPayload, FactRef,
    FactVersionRef, GenerationId, GraphDelta, GraphDeltaWithConsequences, GraphDeltaWithLineage,
    IndexRunId, LineageEndpoint, Node, NodeId, NodeKind, ObservationTime, Provenance, ProvenanceId,
    RelationKind, RepositoryId, WorktreeId,
};
use syntaxmesh_query::{ConsequenceTraceRequest, Query};
use syntaxmesh_store::{
    ConsequenceRangeReadMetrics, GraphStore, HistoricalEdgeReadMetrics, InMemoryGraphStore,
    StoreError,
};
use syntaxmesh_store_sqlite::SqliteGraphStore;
use syntaxmesh_store_turso::TursoGraphStore;

const DEPTHS: [usize; 3] = [64, 256, 1_024];
const GRAPH_SIZES: [usize; 3] = [100, 1_000, 5_000];
const SAMPLES: usize = 5;
const NODE_ITERATIONS: u32 = 50;
const SNAPSHOT_ITERATIONS: u32 = 10;
const RANGE_ITERATIONS: u32 = 50;
const ACCEPTANCE_ITERATIONS: u32 = 50;
const OBSERVATION_ITERATIONS: u32 = 50;
const CHANGE_EVENT_ITERATIONS: u32 = 50;
const NEIGHBOR_PAGE_ITERATIONS: u32 = 50;
const NEIGHBOR_PAGE_SIZE: usize = 10;
const NEIGHBOR_DEGREE: usize = 64;
const CONSEQUENCE_RANGE_ITERATIONS: u32 = 50;
const CONSEQUENCE_RANGE_PAGE_SIZE: usize = 10;

struct BenchmarkDirectory(PathBuf);

impl BenchmarkDirectory {
    fn new() -> Result<Self, std::io::Error> {
        let path = std::env::var_os("SYNTAXMESH_BENCH_DB_DIR").map_or_else(
            || {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .ancestors()
                    .nth(2)
                    .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
                    .join("target/benchmark-scratch")
                    .join(format!("temporal-depth-{}", std::process::id()))
            },
            PathBuf::from,
        );
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
}

impl Drop for BenchmarkDirectory {
    fn drop(&mut self) {
        if std::env::var_os("SYNTAXMESH_BENCH_KEEP").is_some() {
            eprintln!(
                "preserving temporal-depth benchmark databases at {}",
                self.0.display()
            );
            return;
        }
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!(
                "could not remove benchmark fixture {}: {error}",
                self.0.display()
            );
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let fixture = BenchmarkDirectory::new()?;
    for depth in DEPTHS {
        let mut memory: Box<dyn GraphStore> = Box::new(InMemoryGraphStore::new());
        benchmark_store("in-memory", memory.as_mut(), depth)?;

        let sqlite_path = fixture.0.join(format!("sqlite-{depth}.db"));
        SqliteGraphStore::migrate(&sqlite_path)?;
        let mut sqlite: Box<dyn GraphStore> = Box::new(SqliteGraphStore::open(&sqlite_path)?);
        benchmark_store("sqlite", sqlite.as_mut(), depth)?;
        drop(sqlite);
        benchmark_store_open("sqlite", &sqlite_path)?;
        println!(
            "backend=sqlite history_generations={depth} persisted_database_bytes={}",
            persisted_database_bytes(&sqlite_path)?
        );

        let turso_path = fixture.0.join(format!("turso-{depth}.db"));
        TursoGraphStore::migrate(&turso_path)?;
        let mut turso: Box<dyn GraphStore> = Box::new(TursoGraphStore::open(&turso_path)?);
        benchmark_store("turso", turso.as_mut(), depth)?;
        drop(turso);
        benchmark_store_open("turso", &turso_path)?;
        println!(
            "backend=turso history_generations={depth} persisted_database_bytes={}",
            persisted_database_bytes(&turso_path)?
        );
    }
    for graph_size in GRAPH_SIZES {
        let mut memory: Box<dyn GraphStore> = Box::new(InMemoryGraphStore::new());
        let memory_generation = benchmark_graph_size("in-memory", memory.as_mut(), graph_size)?;
        benchmark_publication_scaling("in-memory", memory.as_mut(), graph_size, memory_generation)?;

        let sqlite_path = fixture.0.join(format!("sqlite-graph-{graph_size}.db"));
        SqliteGraphStore::migrate(&sqlite_path)?;
        let mut sqlite: Box<dyn GraphStore> = Box::new(SqliteGraphStore::open(&sqlite_path)?);
        let sqlite_generation = benchmark_graph_size("sqlite", sqlite.as_mut(), graph_size)?;
        let sqlite_fixture_bytes = persisted_database_bytes(&sqlite_path)?;
        benchmark_publication_scaling("sqlite", sqlite.as_mut(), graph_size, sqlite_generation)?;
        drop(sqlite);
        println!(
            "backend=sqlite graph_nodes={graph_size} graph_fixture_db_wal_shm_bytes={sqlite_fixture_bytes} after_publication_scaling_db_wal_shm_bytes={}",
            persisted_database_bytes(&sqlite_path)?,
        );

        let turso_path = fixture.0.join(format!("turso-graph-{graph_size}.db"));
        TursoGraphStore::migrate(&turso_path)?;
        let mut turso: Box<dyn GraphStore> = Box::new(TursoGraphStore::open(&turso_path)?);
        let turso_generation = benchmark_graph_size("turso", turso.as_mut(), graph_size)?;
        let turso_fixture_bytes = persisted_database_bytes(&turso_path)?;
        benchmark_publication_scaling("turso", turso.as_mut(), graph_size, turso_generation)?;
        drop(turso);
        println!(
            "backend=turso graph_nodes={graph_size} graph_fixture_db_wal_shm_bytes={turso_fixture_bytes} after_publication_scaling_db_wal_shm_bytes={}",
            persisted_database_bytes(&turso_path)?,
        );
    }
    Ok(())
}

fn benchmark_store_open(
    backend: &str,
    database_path: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    let open_nanos = sample_median(SAMPLES, 1, || {
        match backend {
            "sqlite" => {
                let store = SqliteGraphStore::open(database_path)
                    .map_err(|error| StoreError::Backend(error.to_string()))?;
                std::hint::black_box(store.latest_generation());
                drop(store);
            }
            "turso" => {
                let store = TursoGraphStore::open(database_path)
                    .map_err(|error| StoreError::Backend(error.to_string()))?;
                std::hint::black_box(store.latest_generation());
                drop(store);
            }
            _ => {
                return Err(StoreError::Backend(format!(
                    "unsupported store-open benchmark backend: {backend}"
                )));
            }
        }
        Ok(())
    })?;
    println!("backend={backend} store_open_median_ns={open_nanos}");
    Ok(())
}

fn persisted_database_bytes(database_path: &std::path::Path) -> Result<u64, std::io::Error> {
    let mut total = fs::metadata(database_path)?.len();
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = database_path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        match fs::metadata(sidecar) {
            Ok(metadata) => {
                total = total.saturating_add(metadata.len());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(total)
}

fn benchmark_graph_size(
    backend: &str,
    store: &mut dyn GraphStore,
    graph_size: usize,
) -> Result<GenerationId, Box<dyn Error>> {
    let generation = populate_graph(store, graph_size)?;
    let snapshot_nanos = sample_median(SAMPLES, 5, || {
        let snapshot = store.historical_snapshot(generation)?;
        if snapshot.nodes.len() != graph_size
            || snapshot.edges.len() != graph_size.saturating_sub(2)
        {
            return Err(StoreError::Integrity(
                "graph-size benchmark output size changed".to_owned(),
            ));
        }
        let _snapshot = std::hint::black_box(snapshot);
        Ok(())
    })?;
    let endpoint = NodeId::derive(&[b"temporal-size-benchmark-node", &0_u64.to_le_bytes()]);
    let (neighbor_page_nanos, neighbor_reads) =
        benchmark_neighbor_page(store, generation, endpoint, true)?;
    validate_neighbor_read_metrics(backend, neighbor_reads)?;
    println!(
        "backend={backend} graph_nodes={graph_size} unrelated_nodes={} fixed_endpoint_degree={NEIGHBOR_DEGREE} graph_edges={} graph_at_median_ns={snapshot_nanos} historical_neighbors_10_median_ns={neighbor_page_nanos} historical_neighbor_sql_read_statements={} historical_neighbor_index_page_lookups={} historical_neighbor_index_pages_loaded={} historical_neighbor_edge_payload_rows_fetched={} historical_neighbor_reference_snapshots_materialized={}",
        graph_size.saturating_sub(NEIGHBOR_DEGREE + 1),
        graph_size.saturating_sub(2),
        neighbor_reads.sql_read_statements,
        neighbor_reads.incidence_index_page_lookups,
        neighbor_reads.incidence_index_pages_loaded,
        neighbor_reads.edge_payload_rows_fetched,
        neighbor_reads.reference_snapshots_materialized,
    );
    Ok(generation)
}

fn benchmark_publication_scaling(
    backend: &str,
    store: &mut dyn GraphStore,
    graph_size: usize,
    mut expected_base: GenerationId,
) -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"temporal-size-benchmark-repository"]);
    let worktree = WorktreeId::derive(&[b"temporal-size-benchmark-worktree"]);
    let samples = 5_usize;
    for changed_fact_count in [1_usize, 10, 100] {
        if changed_fact_count > graph_size {
            continue;
        }
        let mut durations = Vec::with_capacity(samples);
        for sample in 0..samples {
            let sequence = u64::try_from(sample)
                .map_err(|error| StoreError::Backend(error.to_string()))?
                .checked_add(
                    u64::try_from(changed_fact_count)
                        .map_err(|error| StoreError::Backend(error.to_string()))?,
                )
                .and_then(|value| value.checked_add(u64::try_from(graph_size).ok()?))
                .ok_or_else(|| StoreError::Backend("sequence overflowed".to_owned()))?;
            let sequence_bytes = sequence.to_le_bytes();
            let next_generation = GenerationId::derive(&[
                b"temporal-size-benchmark-publication",
                &u64::try_from(graph_size)
                    .map_err(|error| StoreError::Backend(error.to_string()))?
                    .to_le_bytes(),
                &u64::try_from(changed_fact_count)
                    .map_err(|error| StoreError::Backend(error.to_string()))?
                    .to_le_bytes(),
                &sequence_bytes,
            ]);
            let nodes = (0..changed_fact_count)
                .map(|index| {
                    let index_bytes = u64::try_from(index)
                        .map_err(|error| StoreError::Backend(error.to_string()))?
                        .to_le_bytes();
                    Ok(Node {
                        id: NodeId::derive(&[b"temporal-size-benchmark-node", &index_bytes]),
                        kind: NodeKind::Function,
                        name: format!("symbol-{index}-revision-{sequence}"),
                        owner_file: None,
                        source: None,
                        provenance: ProvenanceId::derive(&[b"temporal-size-benchmark-provenance"]),
                        extension_payload: None,
                    })
                })
                .collect::<Result<Vec<_>, StoreError>>()?;
            let delta = GraphDelta {
                repository,
                worktree,
                run_id: IndexRunId::derive(&[
                    b"temporal-size-benchmark-publication-run",
                    &sequence_bytes,
                ]),
                expected_base: Some(expected_base),
                next_generation,
                changed_files: Vec::new(),
                removed_files: Vec::new(),
                upsert_provenance: Vec::new(),
                upsert_nodes: nodes,
                upsert_edges: Vec::new(),
                remove_nodes: Vec::new(),
                remove_edges: Vec::new(),
            };

            let started = Instant::now();
            store.apply_delta(delta)?;
            durations.push(started.elapsed().as_nanos());
            expected_base = next_generation;

            for index in 0..changed_fact_count {
                let index_bytes = u64::try_from(index)
                    .map_err(|error| StoreError::Backend(error.to_string()))?
                    .to_le_bytes();
                let node_id = NodeId::derive(&[b"temporal-size-benchmark-node", &index_bytes]);
                let node = store
                    .historical_node(next_generation, node_id)?
                    .ok_or_else(|| {
                        StoreError::Integrity("publication scaling lost an updated node".to_owned())
                    })?;
                if node.name != format!("symbol-{index}-revision-{sequence}") {
                    return Err(StoreError::Integrity(
                        "publication scaling returned stale node content".to_owned(),
                    )
                    .into());
                }
            }
        }
        durations.sort_unstable();
        let median = durations
            .get(durations.len() / 2)
            .copied()
            .ok_or_else(|| StoreError::Integrity("no publication samples".to_owned()))?;
        println!(
            "backend={backend} existing_graph_nodes={graph_size} existing_graph_facts={} changed_node_facts={changed_fact_count} samples={samples} median_publication_ns={median}",
            graph_size.saturating_mul(2),
        );
    }
    Ok(())
}

fn populate_graph(
    store: &mut dyn GraphStore,
    graph_size: usize,
) -> Result<GenerationId, StoreError> {
    let repository = RepositoryId::derive(&[b"temporal-size-benchmark-repository"]);
    let worktree = WorktreeId::derive(&[b"temporal-size-benchmark-worktree"]);
    let provenance_id = ProvenanceId::derive(&[b"temporal-size-benchmark-provenance"]);
    let generation = GenerationId::derive(&[b"temporal-size-benchmark-generation"]);
    let provenance = Provenance {
        id: provenance_id,
        producer_namespace: "syntaxmesh.temporal-size-benchmark".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let mut nodes = Vec::with_capacity(graph_size);
    let mut edges = Vec::with_capacity(graph_size.saturating_sub(1));
    for index in 0..graph_size {
        let index_bytes = u64::try_from(index)
            .map_err(|error| StoreError::Backend(error.to_string()))?
            .to_le_bytes();
        let id = NodeId::derive(&[b"temporal-size-benchmark-node", &index_bytes]);
        nodes.push(Node {
            id,
            kind: NodeKind::Function,
            name: format!("symbol-{index}"),
            owner_file: None,
            source: None,
            provenance: provenance_id,
            extension_payload: None,
        });
        if (1..=NEIGHBOR_DEGREE).contains(&index) {
            edges.push(Edge {
                id: EdgeId::derive(&[b"temporal-size-benchmark-edge", &index_bytes]),
                source: NodeId::derive(&[b"temporal-size-benchmark-node", &0_u64.to_le_bytes()]),
                target: id,
                relation: RelationKind::Calls,
                provenance: provenance_id,
                extension_payload: None,
            });
        } else if index >= NEIGHBOR_DEGREE + 2 {
            let previous_index = index.checked_sub(1).ok_or_else(|| {
                StoreError::Integrity("graph-size previous index underflowed".to_owned())
            })?;
            let previous_bytes = u64::try_from(previous_index)
                .map_err(|error| StoreError::Backend(error.to_string()))?
                .to_le_bytes();
            edges.push(Edge {
                id: EdgeId::derive(&[b"temporal-size-benchmark-edge", &index_bytes]),
                source: NodeId::derive(&[b"temporal-size-benchmark-node", &previous_bytes]),
                target: id,
                relation: RelationKind::Calls,
                provenance: provenance_id,
                extension_payload: None,
            });
        }
    }
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"temporal-size-benchmark-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: nodes,
        upsert_edges: edges,
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    Ok(generation)
}

fn benchmark_store(
    backend: &str,
    store: &mut dyn GraphStore,
    depth: usize,
) -> Result<(), Box<dyn Error>> {
    let generations = populate(store, depth)?;
    let consequence_from = generations.first().copied().ok_or_else(|| {
        StoreError::Integrity("consequence range start generation is missing".to_owned())
    })?;
    let consequence_until = generations.last().copied().ok_or_else(|| {
        StoreError::Integrity("consequence range end generation is missing".to_owned())
    })?;
    let target_index = depth.saturating_sub(2);
    let target = generations.get(target_index).copied().ok_or_else(|| {
        StoreError::Integrity("benchmark target generation is missing".to_owned())
    })?;
    let from_index = target_index.saturating_sub(10);
    let from = generations
        .get(from_index)
        .copied()
        .ok_or_else(|| StoreError::Integrity("benchmark range start is missing".to_owned()))?;
    let node_id = NodeId::derive(&[b"temporal-depth-benchmark-node"]);
    let observed_from = ObservationTime(
        u64::try_from(depth.saturating_sub(10))
            .map_err(|error| StoreError::Backend(error.to_string()))?,
    );
    let observed_until = ObservationTime(
        u64::try_from(depth).map_err(|error| StoreError::Backend(error.to_string()))?,
    );

    let node_nanos = sample_median(SAMPLES, NODE_ITERATIONS, || {
        let node = store.historical_node(target, node_id)?;
        if node.is_none() {
            return Err(StoreError::Integrity(
                "historical node query returned no node".to_owned(),
            ));
        }
        let _node = std::hint::black_box(node);
        Ok(())
    })?;
    let snapshot_nanos = sample_median(SAMPLES, SNAPSHOT_ITERATIONS, || {
        let snapshot = store.historical_snapshot(target)?;
        if snapshot.nodes.len() != 2 + NEIGHBOR_DEGREE {
            return Err(StoreError::Integrity(
                "historical snapshot output size changed".to_owned(),
            ));
        }
        let _snapshot = std::hint::black_box(snapshot);
        Ok(())
    })?;
    let range_nanos = sample_median(SAMPLES, RANGE_ITERATIONS, || {
        let changes = store.changes_between(from, target)?;
        if changes.len() != 10 {
            return Err(StoreError::Integrity(
                "benchmark diff result size changed".to_owned(),
            ));
        }
        let _changes = std::hint::black_box(changes);
        Ok(())
    })?;
    let from_time = AcceptanceTime(
        u64::try_from(depth.saturating_sub(9))
            .map_err(|error| StoreError::Backend(error.to_string()))?,
    );
    let until_time = AcceptanceTime(
        u64::try_from(depth.saturating_add(1))
            .map_err(|error| StoreError::Backend(error.to_string()))?,
    );
    let acceptance_nanos = sample_median(SAMPLES, ACCEPTANCE_ITERATIONS, || {
        let accepted = store.accepted_generations_between(from_time, until_time, None, 10)?;
        if accepted.items.len() != 10 || accepted.next_cursor.is_some() {
            return Err(StoreError::Integrity(
                "acceptance timeline output size changed".to_owned(),
            ));
        }
        let _accepted = std::hint::black_box(accepted);
        Ok(())
    })?;
    let observation_nanos = sample_median(SAMPLES, OBSERVATION_ITERATIONS, || {
        let observed = store.observed_facts_between(observed_from, observed_until, None, 10)?;
        if observed.items.len() != 10 || observed.next_cursor.is_some() {
            return Err(StoreError::Integrity(
                "observation timeline output size changed".to_owned(),
            ));
        }
        let _observed = std::hint::black_box(observed);
        Ok(())
    })?;
    let (neighbor_page_nanos, neighbor_reads) =
        benchmark_neighbor_page(store, target, node_id, true)?;
    validate_neighbor_read_metrics(backend, neighbor_reads)?;
    let change_event_nanos = sample_median(SAMPLES, CHANGE_EVENT_ITERATIONS, || {
        let events = store.change_events_for_fact(FactRef::Node(node_id), None, 10)?;
        if events.items.len() != 10 || events.next_cursor.is_none() {
            return Err(StoreError::Integrity(
                "change-event page output size changed".to_owned(),
            ));
        }
        let _events = std::hint::black_box(events);
        Ok(())
    })?;
    let consequence_endpoint = LineageEndpoint::FactVersion(FactVersionRef {
        fact: FactRef::Node(node_id),
        valid_from: consequence_from,
    });
    let (consequence_range_nanos, consequence_reads) = benchmark_consequence_range(
        store,
        consequence_endpoint,
        consequence_from,
        consequence_until,
    )?;
    validate_consequence_range_metrics(backend, depth, consequence_reads)?;
    let trace_origin = LineageEndpoint::FactVersion(FactVersionRef {
        fact: FactRef::Node(node_id),
        valid_from: consequence_from,
    });
    let (consequence_trace_nanos, consequence_trace) = benchmark_consequence_trace(
        store,
        consequence_until,
        trace_origin,
        consequence_from,
        consequence_until,
    )?;

    println!(
        "backend={backend} history_generations={depth} graph_nodes={} diff_generations=10 node_at_median_ns={node_nanos} graph_at_median_ns={snapshot_nanos} changed_between_median_ns={range_nanos} accepted_between_10_median_ns={acceptance_nanos} observed_between_10_median_ns={observation_nanos} change_events_10_median_ns={change_event_nanos} consequence_range_1_median_ns={consequence_range_nanos} consequence_range_sql_read_statements={} consequence_range_reference_generation_entries_materialized={} consequence_range_reference_consequence_entries_materialized={} consequence_range_reference_consequence_entries_scanned={} consequence_range_reference_mutations_scanned={} consequence_range_rows_returned={} consequence_trace_2_hop_median_ns={consequence_trace_nanos} consequence_trace_range_pages={} consequence_trace_generation_sequence_lookups={} consequence_trace_range_page_sql_read_statements={} consequence_trace_reference_generation_entries_materialized={} consequence_trace_reference_consequence_entries_materialized={} consequence_trace_reference_consequence_entries_scanned={} consequence_trace_reference_mutations_scanned={} consequence_trace_rows_returned={} consequence_trace_hops={} consequence_trace_scanned_incidences={} historical_neighbors_10_median_ns={neighbor_page_nanos} historical_neighbor_sql_read_statements={} historical_neighbor_index_page_lookups={} historical_neighbor_index_pages_loaded={} historical_neighbor_edge_payload_rows_fetched={} historical_neighbor_reference_snapshots_materialized={}",
        2 + NEIGHBOR_DEGREE,
        consequence_reads.sql_read_statements,
        consequence_reads.reference_generation_entries_materialized,
        consequence_reads.reference_consequence_entries_materialized,
        consequence_reads.reference_consequence_entries_scanned,
        consequence_reads.reference_consequence_mutations_scanned,
        consequence_reads.consequence_rows_returned,
        consequence_trace.read_metrics.endpoint_range_pages,
        consequence_trace.read_metrics.generation_sequence_lookups,
        consequence_trace
            .read_metrics
            .range_page_reads
            .sql_read_statements,
        consequence_trace
            .read_metrics
            .range_page_reads
            .reference_generation_entries_materialized,
        consequence_trace
            .read_metrics
            .range_page_reads
            .reference_consequence_entries_materialized,
        consequence_trace
            .read_metrics
            .range_page_reads
            .reference_consequence_entries_scanned,
        consequence_trace
            .read_metrics
            .range_page_reads
            .reference_consequence_mutations_scanned,
        consequence_trace
            .read_metrics
            .range_page_reads
            .consequence_rows_returned,
        consequence_trace.hops.len(),
        consequence_trace.scanned_incidences,
        neighbor_reads.sql_read_statements,
        neighbor_reads.incidence_index_page_lookups,
        neighbor_reads.incidence_index_pages_loaded,
        neighbor_reads.edge_payload_rows_fetched,
        neighbor_reads.reference_snapshots_materialized,
    );
    Ok(())
}

fn benchmark_consequence_trace(
    store: &dyn GraphStore,
    pinned_generation: GenerationId,
    origin: LineageEndpoint,
    from: GenerationId,
    until: GenerationId,
) -> Result<(u128, syntaxmesh_query::ConsequenceTrace), Box<dyn Error>> {
    let request = ConsequenceTraceRequest {
        origin,
        from_generation: from,
        until_generation: until,
        max_hops: 2,
        max_endpoints: 10,
        max_edges: 10,
        max_scanned_incidences: 10,
        included_kinds: Vec::new(),
        included_evidence_classes: Vec::new(),
    };
    let query = Query::new(store, pinned_generation);
    let median = sample_median(SAMPLES, CONSEQUENCE_RANGE_ITERATIONS, || {
        let trace = query.consequence_trace(&request).map_err(|error| {
            StoreError::Backend(format!("consequence trace benchmark failed: {error}"))
        })?;
        if trace.hops.len() != 2 || trace.states.len() != 3 || trace.truncated {
            return Err(StoreError::Integrity(
                "consequence-trace benchmark result changed shape".to_owned(),
            ));
        }
        let _trace = std::hint::black_box(trace);
        Ok(())
    })?;
    let trace = query.consequence_trace(&request).map_err(|error| {
        StoreError::Backend(format!("consequence trace metric probe failed: {error}"))
    })?;
    if trace.hops.len() != 2 || trace.states.len() != 3 || trace.truncated {
        return Err(StoreError::Integrity(
            "consequence-trace metric probe changed result shape".to_owned(),
        )
        .into());
    }
    if trace.read_metrics.generation_sequence_lookups != 2 {
        return Err(StoreError::Integrity(format!(
            "consequence-trace generation-sequence cache missed repeated keys: {:?}",
            trace.read_metrics
        ))
        .into());
    }
    Ok((median, trace))
}

fn benchmark_neighbor_page(
    store: &dyn GraphStore,
    generation: GenerationId,
    endpoint: NodeId,
    expected_has_more: bool,
) -> Result<(u128, HistoricalEdgeReadMetrics), Box<dyn Error>> {
    let median = sample_median(SAMPLES, NEIGHBOR_PAGE_ITERATIONS, || {
        let page = store.historical_incident_edges(
            generation,
            endpoint,
            EdgeDirection::Outgoing,
            None,
            NEIGHBOR_PAGE_SIZE,
        )?;
        if page.items.len() != NEIGHBOR_PAGE_SIZE
            || page.has_more != expected_has_more
            || page.items.windows(2).any(|pair| {
                pair.first()
                    .zip(pair.get(1))
                    .is_some_and(|(a, b)| a.id >= b.id)
            })
        {
            return Err(StoreError::Integrity(
                "historical-neighbor benchmark result changed shape or ordering".to_owned(),
            ));
        }
        let _page = std::hint::black_box(page);
        Ok(())
    })?;
    let page = store.historical_incident_edges(
        generation,
        endpoint,
        EdgeDirection::Outgoing,
        None,
        NEIGHBOR_PAGE_SIZE,
    )?;
    if page.items.len() != NEIGHBOR_PAGE_SIZE || page.has_more != expected_has_more {
        return Err(StoreError::Integrity(
            "historical-neighbor metric probe changed result shape".to_owned(),
        )
        .into());
    }
    Ok((median, page.read_metrics))
}

fn benchmark_consequence_range(
    store: &dyn GraphStore,
    endpoint: LineageEndpoint,
    from: GenerationId,
    until: GenerationId,
) -> Result<(u128, ConsequenceRangeReadMetrics), Box<dyn Error>> {
    let median = sample_median(SAMPLES, CONSEQUENCE_RANGE_ITERATIONS, || {
        let page = store.consequence_edges_for_endpoint_range(
            endpoint,
            from,
            until,
            None,
            CONSEQUENCE_RANGE_PAGE_SIZE,
        )?;
        if page.items.len() != 1 || page.next_cursor.is_some() {
            return Err(StoreError::Integrity(
                "consequence-range benchmark result changed shape".to_owned(),
            ));
        }
        let _page = std::hint::black_box(page);
        Ok(())
    })?;
    let page = store.consequence_edges_for_endpoint_range(
        endpoint,
        from,
        until,
        None,
        CONSEQUENCE_RANGE_PAGE_SIZE,
    )?;
    if page.items.len() != 1 || page.next_cursor.is_some() {
        return Err(StoreError::Integrity(
            "consequence-range metric probe changed result shape".to_owned(),
        )
        .into());
    }
    Ok((median, page.read_metrics))
}

fn validate_consequence_range_metrics(
    backend: &str,
    depth: usize,
    metrics: ConsequenceRangeReadMetrics,
) -> Result<(), StoreError> {
    let valid = if backend == "in-memory" {
        metrics.sql_read_statements == 0
            && metrics.reference_generation_entries_materialized == depth
            && metrics.reference_consequence_entries_materialized == depth
            && metrics.reference_consequence_entries_scanned == depth
            && metrics.reference_consequence_mutations_scanned == 2
            && metrics.consequence_rows_returned == 1
    } else {
        metrics.sql_read_statements == 1
            && metrics.reference_generation_entries_materialized == 0
            && metrics.reference_consequence_entries_materialized == 0
            && metrics.reference_consequence_entries_scanned == 0
            && metrics.reference_consequence_mutations_scanned == 0
            && metrics.consequence_rows_returned == 1
    };
    if !valid {
        return Err(StoreError::Integrity(format!(
            "consequence-range instrumentation is inconsistent for {backend} at depth {depth}: {metrics:?}"
        )));
    }
    Ok(())
}

fn validate_neighbor_read_metrics(
    backend: &str,
    metrics: HistoricalEdgeReadMetrics,
) -> Result<(), StoreError> {
    let valid = if backend == "in-memory" {
        metrics.reference_snapshots_materialized == 1
            && metrics.sql_read_statements == 0
            && metrics.incidence_index_page_lookups == 0
            && metrics.incidence_index_pages_loaded == 0
            && metrics.edge_payload_rows_fetched == 0
    } else {
        metrics.reference_snapshots_materialized == 0
            && metrics.sql_read_statements
                == 2_usize
                    .saturating_add(metrics.incidence_index_pages_loaded)
                    .saturating_add(metrics.edge_payload_rows_fetched)
            && metrics.incidence_index_page_lookups >= metrics.incidence_index_pages_loaded
            && metrics.incidence_index_page_lookups > 0
            && metrics.incidence_index_pages_loaded > 0
            && metrics.edge_payload_rows_fetched == NEIGHBOR_PAGE_SIZE
    };
    if !valid {
        return Err(StoreError::Integrity(format!(
            "historical-neighbor instrumentation is inconsistent for {backend}: {metrics:?}"
        )));
    }
    Ok(())
}

fn sample_median(
    samples: usize,
    iterations: u32,
    mut operation: impl FnMut() -> Result<(), StoreError>,
) -> Result<u128, StoreError> {
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        for _ in 0..iterations {
            operation()?;
        }
        let divisor = u128::from(iterations);
        let per_query = started
            .elapsed()
            .as_nanos()
            .checked_div(divisor)
            .ok_or_else(|| StoreError::Integrity("benchmark iteration count is zero".to_owned()))?;
        durations.push(per_query);
    }
    durations.sort_unstable();
    let median_index = durations
        .len()
        .checked_div(2)
        .ok_or_else(|| StoreError::Integrity("benchmark produced no timing samples".to_owned()))?;
    durations
        .get(median_index)
        .copied()
        .ok_or_else(|| StoreError::Integrity("benchmark produced no timing samples".to_owned()))
}

fn populate(store: &mut dyn GraphStore, depth: usize) -> Result<Vec<GenerationId>, StoreError> {
    let repository = RepositoryId::derive(&[b"temporal-depth-benchmark-repository"]);
    let worktree = WorktreeId::derive(&[b"temporal-depth-benchmark-worktree"]);
    let node_id = NodeId::derive(&[b"temporal-depth-benchmark-node"]);
    let provenance_id = ProvenanceId::derive(&[b"temporal-depth-benchmark-provenance"]);
    let observation_provenance_id =
        ProvenanceId::derive(&[b"temporal-depth-benchmark-observation-provenance"]);
    let observation_node_id = NodeId::derive(&[b"temporal-depth-benchmark-observation"]);
    let mut generations = Vec::with_capacity(depth);
    for sequence in 1..=depth {
        let sequence_value =
            u64::try_from(sequence).map_err(|error| StoreError::Backend(error.to_string()))?;
        let sequence_bytes = sequence_value.to_le_bytes();
        let generation = GenerationId::derive(&[b"temporal-depth-benchmark", &sequence_bytes]);
        let provenance = Provenance {
            id: provenance_id,
            producer_namespace: "syntaxmesh.temporal-benchmark".to_owned(),
            producer_version: "1".to_owned(),
            evidence_class: EvidenceClass::SourceFact,
            source: None,
        };
        let observation_provenance = Provenance {
            id: observation_provenance_id,
            producer_namespace: "syntaxmesh.temporal-observation-benchmark".to_owned(),
            producer_version: "1".to_owned(),
            evidence_class: EvidenceClass::RuntimeObserved,
            source: None,
        };
        let node = Node {
            id: node_id,
            kind: NodeKind::Function,
            name: format!("symbol-version-{sequence}"),
            owner_file: None,
            source: None,
            provenance: provenance_id,
            extension_payload: None,
        };
        let observation_node = Node {
            id: observation_node_id,
            kind: NodeKind::RuntimeObservation,
            name: "runtime-observation-version".to_owned(),
            owner_file: None,
            source: None,
            provenance: observation_provenance_id,
            extension_payload: Some(ExtensionPayload {
                namespace: "syntaxmesh.temporal-observation-benchmark".to_owned(),
                schema_version: 1,
                bytes: format!("{{\"observed_at_unix_nanos\":{sequence_value}}}").into_bytes(),
            }),
        };
        let mut upsert_nodes = vec![node, observation_node];
        let mut upsert_edges = Vec::new();
        if sequence == 1 {
            upsert_nodes.reserve(NEIGHBOR_DEGREE);
            upsert_edges.reserve(NEIGHBOR_DEGREE);
            for index in 0..NEIGHBOR_DEGREE {
                let index_bytes = u64::try_from(index)
                    .map_err(|error| StoreError::Backend(error.to_string()))?
                    .to_le_bytes();
                let neighbor_id =
                    NodeId::derive(&[b"temporal-depth-benchmark-neighbor", &index_bytes]);
                upsert_nodes.push(Node {
                    id: neighbor_id,
                    kind: NodeKind::Function,
                    name: format!("neighbor-{index}"),
                    owner_file: None,
                    source: None,
                    provenance: provenance_id,
                    extension_payload: None,
                });
                upsert_edges.push(Edge {
                    id: EdgeId::derive(&[b"temporal-depth-benchmark-edge", &index_bytes]),
                    source: node_id,
                    target: neighbor_id,
                    relation: RelationKind::Calls,
                    provenance: provenance_id,
                    extension_payload: None,
                });
            }
        }
        let previous = generations.last().copied();
        let graph = GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"temporal-depth-benchmark-run", &sequence_bytes]),
            expected_base: previous,
            next_generation: generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: if sequence == 1 {
                vec![provenance, observation_provenance]
            } else {
                Vec::new()
            },
            upsert_nodes,
            upsert_edges,
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        };
        if sequence == 1 {
            let consequence_source = FactVersionRef {
                fact: FactRef::Node(node_id),
                valid_from: generation,
            };
            let neighbor_id =
                NodeId::derive(&[b"temporal-depth-benchmark-neighbor", &0_u64.to_le_bytes()]);
            let consequence_target = FactVersionRef {
                fact: FactRef::Node(neighbor_id),
                valid_from: generation,
            };
            let next_neighbor_id =
                NodeId::derive(&[b"temporal-depth-benchmark-neighbor", &1_u64.to_le_bytes()]);
            let next_consequence_target = FactVersionRef {
                fact: FactRef::Node(next_neighbor_id),
                valid_from: generation,
            };
            let edge = |name: &[u8], source, target| ConsequenceEdge {
                id: ConsequenceEdgeId::derive(&[name]),
                source: LineageEndpoint::FactVersion(source),
                target: LineageEndpoint::FactVersion(target),
                kind: ConsequenceKind::DirectDependencyEffect,
                evidence: vec![source, target],
                derivation: ConsequenceDerivation::Explicit,
                provenance: provenance_id,
            };
            store.apply_delta_with_consequences(
                GraphDeltaWithConsequences {
                    publication: GraphDeltaWithLineage {
                        graph,
                        lineage: Default::default(),
                    },
                    consequences: ConsequenceDelta {
                        add: vec![
                            edge(
                                b"temporal-depth-benchmark-consequence-1",
                                consequence_source,
                                consequence_target,
                            ),
                            edge(
                                b"temporal-depth-benchmark-consequence-2",
                                consequence_target,
                                next_consequence_target,
                            ),
                        ],
                        retract: Vec::new(),
                    },
                },
                Some(AcceptanceTime(sequence_value)),
            )?;
        } else {
            store.apply_delta_at(graph, AcceptanceTime(sequence_value))?;
        }
        generations.push(generation);
    }
    Ok(generations)
}
