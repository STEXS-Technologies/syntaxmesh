use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use syntaxmesh_core::{GenerationId, GraphSnapshot, IndexRunId, RepositoryId, WorktreeId};
use syntaxmesh_engine::{Clock, SyntaxMeshEngine, SystemClock};
use syntaxmesh_indexer::Indexer;
use syntaxmesh_integration_penelope::PenelopePublisher;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_scanner::scan;
use syntaxmesh_store::{GraphStore, InMemoryGraphStore, graph_snapshot_root_v2};
use syntaxmesh_store_sqlite::SqliteGraphStore;
use syntaxmesh_store_turso::TursoGraphStore;
use syntaxmesh_workflow::DurableIndexWorkflow;

struct BenchmarkDirectory(PathBuf);

struct RepositoryFixture<'sources> {
    initial_sources: &'sources [syntaxmesh_language_sdk::SourceFile],
    changed_sources: &'sources [syntaxmesh_language_sdk::SourceFile],
    initial_input_bytes: u64,
    incremental_input_bytes: u64,
    repository: RepositoryId,
    worktree: WorktreeId,
}

impl BenchmarkDirectory {
    fn new() -> Result<Self, std::io::Error> {
        let path = std::env::var_os("SYNTAXMESH_BENCH_DB_DIR").map_or_else(
            || {
                std::env::temp_dir().join(format!(
                    "syntaxmesh-repository-indexing-bench-{}",
                    std::process::id()
                ))
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
                "preserving repository benchmark databases at {}",
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

#[derive(Default)]
struct MemoryTracker {
    previous_peak_kib: Option<u64>,
}

impl MemoryTracker {
    fn new() -> Self {
        Self {
            previous_peak_kib: process_memory_kib().map(|(_, peak)| peak),
        }
    }

    fn report(&mut self, backend: &str, stage: &str) {
        if std::env::var_os("SYNTAXMESH_BENCH_MEMORY_STAGES").is_none() {
            return;
        }
        let Some((rss_kib, peak_rss_kib)) = process_memory_kib() else {
            return;
        };
        let peak_delta_kib = self
            .previous_peak_kib
            .map_or(0, |previous| peak_rss_kib.saturating_sub(previous));
        self.previous_peak_kib = Some(peak_rss_kib);
        println!(
            "resource backend={backend} stage={stage} rss_kib={rss_kib} peak_rss_kib={peak_rss_kib} peak_delta_kib={peak_delta_kib}"
        );
    }
}

fn process_memory_kib() -> Option<(u64, u64)> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let mut rss_kib = None;
    let mut peak_rss_kib = None;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("VmRSS:") {
            rss_kib = value.split_whitespace().next()?.parse().ok();
        } else if let Some(value) = line.strip_prefix("VmHWM:") {
            peak_rss_kib = value.split_whitespace().next()?.parse().ok();
        }
    }
    Some((rss_kib?, peak_rss_kib?))
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = repository_root()?;
    let scan = scan(&root, &["rs"])?;
    if scan.files.is_empty() {
        return Err(format!("no Rust source files found under {}", root.display()).into());
    }
    let source_bytes = scan
        .files
        .iter()
        .map(|source| u64::try_from(source.content.len()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
        .ok_or("source byte count overflowed")?;
    println!(
        "repository={} rust_files={} source_bytes={} scan_elapsed_us={}",
        root.display(),
        scan.metrics.files_read,
        source_bytes,
        scan.metrics.elapsed.as_micros()
    );

    let repository = RepositoryId::derive(&[root.as_os_str().as_encoded_bytes()]);
    let worktree = WorktreeId::derive(&[b"repository-indexing-benchmark-worktree"]);
    let mut changed_sources = scan.files.clone();
    let changed = changed_sources
        .first_mut()
        .ok_or("scanned repository unexpectedly has no source file")?;
    changed
        .content
        .push_str("\n// SyntaxMesh representative incremental benchmark change.\n");
    changed.file.content_hash = *blake3::hash(changed.content.as_bytes()).as_bytes();
    changed.file.size_bytes = u64::try_from(changed.content.len())?;
    let changed_bytes = u64::try_from(changed.content.len())?;
    let mut memory_tracker = MemoryTracker::new();
    memory_tracker.report("scanner", "after-scan-and-fixture-setup");
    let selected_backend =
        std::env::var("SYNTAXMESH_BENCH_BACKEND").unwrap_or_else(|_| "all".to_owned());
    if !matches!(
        selected_backend.as_str(),
        "all" | "in-memory" | "sqlite" | "turso"
    ) {
        return Err(format!(
            "unsupported benchmark backend {selected_backend:?}; expected all, in-memory, sqlite, or turso"
        )
        .into());
    }

    if let Some(stage) = std::env::var_os("SYNTAXMESH_BENCH_STAGE") {
        let backend = std::env::var("SYNTAXMESH_BENCH_BACKEND")?;
        let fixture = BenchmarkDirectory::new()?;
        let source_fixture = RepositoryFixture {
            initial_sources: &scan.files,
            changed_sources: &changed_sources,
            initial_input_bytes: source_bytes,
            incremental_input_bytes: changed_bytes,
            repository,
            worktree,
        };
        benchmark_write_stage(
            &backend,
            stage.to_string_lossy().as_ref(),
            &fixture.0,
            &source_fixture,
        )?;
        memory_tracker.report(&backend, stage.to_string_lossy().as_ref());
        return Ok(());
    }

    if matches!(selected_backend.as_str(), "all" | "in-memory") {
        let memory = InMemoryGraphStore::new();
        benchmark_backend(
            "in-memory",
            memory,
            &scan.files,
            &changed_sources,
            repository,
            worktree,
            &mut memory_tracker,
        )?;
    }

    if matches!(selected_backend.as_str(), "all" | "sqlite" | "turso") {
        let fixture = BenchmarkDirectory::new()?;
        if matches!(selected_backend.as_str(), "all" | "sqlite") {
            let sqlite_path = fixture.0.join("repository.db");
            SqliteGraphStore::migrate(&sqlite_path)?;
            let sqlite = SqliteGraphStore::open(&sqlite_path)?;
            benchmark_backend(
                "sqlite",
                sqlite,
                &scan.files,
                &changed_sources,
                repository,
                worktree,
                &mut memory_tracker,
            )?;
            println!(
                "backend=sqlite persisted_database_bytes={}",
                persisted_database_bytes(&sqlite_path)?
            );
        }

        if matches!(selected_backend.as_str(), "all" | "turso") {
            let turso_path = fixture.0.join("repository-turso.db");
            TursoGraphStore::migrate(&turso_path)?;
            let turso = TursoGraphStore::open(&turso_path)?;
            benchmark_backend(
                "turso",
                turso,
                &scan.files,
                &changed_sources,
                repository,
                worktree,
                &mut memory_tracker,
            )?;
            println!(
                "backend=turso persisted_database_bytes={}",
                persisted_database_bytes(&turso_path)?
            );
        }
    }
    println!(
        "logical_input_bytes_per_generation=initial:{} incremental:{} (not physical write amplification)",
        source_bytes, changed_bytes
    );
    Ok(())
}

fn repository_root() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(root) = std::env::var_os("SYNTAXMESH_BENCH_ROOT") {
        return Ok(PathBuf::from(root).canonicalize()?);
    }
    for candidate in Path::new(env!("CARGO_MANIFEST_DIR")).ancestors() {
        let manifest = candidate.join("Cargo.toml");
        if fs::read_to_string(&manifest)
            .is_ok_and(|contents| contents.lines().any(|line| line.trim() == "[workspace]"))
        {
            return Ok(candidate.canonicalize()?);
        }
    }
    Err("could not locate the SyntaxMesh workspace root".into())
}

fn benchmark_backend<S>(
    backend: &str,
    store: S,
    initial_sources: &[syntaxmesh_language_sdk::SourceFile],
    changed_sources: &[syntaxmesh_language_sdk::SourceFile],
    repository: RepositoryId,
    worktree: WorktreeId,
    memory_tracker: &mut MemoryTracker,
) -> Result<(), Box<dyn Error>>
where
    S: GraphStore + syntaxmesh_store::DurableRecordStore,
{
    let mut indexer = Indexer::new(store, RustExtractor, repository, worktree);
    let initial_timings = benchmark_index(
        &mut indexer,
        initial_sources,
        IndexRunId::derive(&[b"repository-benchmark-initial-run"]),
        GenerationId::derive(&[b"repository-benchmark-generation-1"]),
        "initial",
        backend,
        memory_tracker,
    )?;
    memory_tracker.report(backend, "initial-index");
    let _initial_status = checked_status(indexer.store(), repository, worktree)?;
    memory_tracker.report(backend, "initial-status");

    let incremental_timings = benchmark_index(
        &mut indexer,
        changed_sources,
        IndexRunId::derive(&[b"repository-benchmark-incremental-run"]),
        GenerationId::derive(&[b"repository-benchmark-generation-2"]),
        "incremental",
        backend,
        memory_tracker,
    )?;
    memory_tracker.report(backend, "incremental-index");
    let incremental_status = checked_status(indexer.store(), repository, worktree)?;
    memory_tracker.report(backend, "incremental-status");
    println!(
        "backend={backend} initial_index_ms={} incremental_one_file_ms={} files={} nodes={} edges={} provenance={}",
        initial_timings.total.as_millis(),
        incremental_timings.total.as_millis(),
        incremental_status.files,
        incremental_status.nodes,
        incremental_status.edges,
        incremental_status.provenance
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct IndexTimings {
    total: std::time::Duration,
}

fn benchmark_index<S>(
    indexer: &mut Indexer<S, RustExtractor>,
    files: &[syntaxmesh_language_sdk::SourceFile],
    run_id: IndexRunId,
    generation: GenerationId,
    phase: &str,
    backend: &str,
    memory_tracker: &mut MemoryTracker,
) -> Result<IndexTimings, Box<dyn Error>>
where
    S: GraphStore + syntaxmesh_store::DurableRecordStore,
{
    let total_started = Instant::now();
    let recovery_started = Instant::now();
    PenelopePublisher::new(indexer.store_mut()).recover_pending()?;
    let recovery = recovery_started.elapsed();

    let preparation_started = Instant::now();
    let delta = indexer.prepare_delta(files, run_id, generation)?;
    let preparation = preparation_started.elapsed();
    memory_tracker.report(backend, &format!("{phase}-prepare-delta"));

    let publication_started = Instant::now();
    let accepted_at = SystemClock.now().map_err(std::io::Error::other)?;
    PenelopePublisher::new(indexer.store_mut()).publish(delta, accepted_at)?;
    let publication = publication_started.elapsed();
    memory_tracker.report(backend, &format!("{phase}-penelope-publish"));

    let total = total_started.elapsed();
    println!(
        "profile backend={backend} phase={phase} recovery_ms={} prepare_delta_ms={} penelope_publish_ms={} total_index_ms={}",
        recovery.as_millis(),
        preparation.as_millis(),
        publication.as_millis(),
        total.as_millis()
    );
    Ok(IndexTimings { total })
}

struct CheckedStatus {
    files: usize,
    nodes: usize,
    edges: usize,
    provenance: usize,
}

fn checked_status<S: GraphStore>(
    store: &S,
    repository: RepositoryId,
    worktree: WorktreeId,
) -> Result<CheckedStatus, Box<dyn Error>> {
    let manifest = store
        .current_generation(repository, worktree)?
        .ok_or("indexing did not publish a generation")?;
    let snapshot = GraphSnapshot {
        files: store.files(manifest.generation)?,
        provenance: store.provenance(manifest.generation)?,
        nodes: store.nodes(manifest.generation)?,
        edges: store.edges(manifest.generation)?,
    };
    if graph_snapshot_root_v2(manifest.generation, &snapshot)? != manifest.graph_root {
        return Err("benchmark graph root failed integrity validation".into());
    }
    let provenance_ids = snapshot
        .provenance
        .iter()
        .map(|item| item.id)
        .collect::<std::collections::BTreeSet<_>>();
    let node_ids = snapshot
        .nodes
        .iter()
        .map(|node| node.id)
        .collect::<std::collections::BTreeSet<_>>();
    let references_valid = snapshot
        .nodes
        .iter()
        .all(|node| provenance_ids.contains(&node.provenance))
        && snapshot.edges.iter().all(|edge| {
            provenance_ids.contains(&edge.provenance)
                && node_ids.contains(&edge.source)
                && node_ids.contains(&edge.target)
        });
    if !references_valid {
        return Err("benchmark graph references failed integrity validation".into());
    }
    Ok(CheckedStatus {
        files: snapshot.files.len(),
        nodes: snapshot.nodes.len(),
        edges: snapshot.edges.len(),
        provenance: snapshot.provenance.len(),
    })
}

fn benchmark_write_stage(
    backend: &str,
    stage: &str,
    database_dir: &Path,
    fixture: &RepositoryFixture<'_>,
) -> Result<(), Box<dyn Error>> {
    let database_path = match backend {
        "sqlite" => database_dir.join("repository.db"),
        "turso" => database_dir.join("repository-turso.db"),
        _ => return Err(format!("unsupported write-benchmark backend: {backend}").into()),
    };
    match (backend, stage) {
        ("sqlite", "bootstrap") => SqliteGraphStore::migrate(&database_path)?,
        ("turso", "bootstrap") => TursoGraphStore::migrate(&database_path)?,
        ("sqlite", "initial") => benchmark_index_stage(
            backend,
            stage,
            SqliteGraphStore::open(&database_path)?,
            fixture.initial_sources,
            fixture.initial_input_bytes,
            fixture.repository,
            fixture.worktree,
        )?,
        ("turso", "initial") => benchmark_index_stage(
            backend,
            stage,
            TursoGraphStore::open(&database_path)?,
            fixture.initial_sources,
            fixture.initial_input_bytes,
            fixture.repository,
            fixture.worktree,
        )?,
        ("sqlite", "incremental") => benchmark_index_stage(
            backend,
            stage,
            SqliteGraphStore::open(&database_path)?,
            fixture.changed_sources,
            fixture.incremental_input_bytes,
            fixture.repository,
            fixture.worktree,
        )?,
        ("turso", "incremental") => benchmark_index_stage(
            backend,
            stage,
            TursoGraphStore::open(&database_path)?,
            fixture.changed_sources,
            fixture.incremental_input_bytes,
            fixture.repository,
            fixture.worktree,
        )?,
        _ => return Err(format!("unsupported write-benchmark stage: {stage}").into()),
    }
    Ok(())
}

fn benchmark_index_stage<S>(
    backend: &str,
    stage: &str,
    store: S,
    sources: &[syntaxmesh_language_sdk::SourceFile],
    logical_input_bytes: u64,
    repository: RepositoryId,
    worktree: WorktreeId,
) -> Result<(), Box<dyn Error>>
where
    S: GraphStore + syntaxmesh_store::DurableRecordStore,
{
    let mut engine = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);
    let (run_id, generation) = match stage {
        "initial" => (
            IndexRunId::derive(&[b"repository-benchmark-initial-run"]),
            GenerationId::derive(&[b"repository-benchmark-generation-1"]),
        ),
        "incremental" => (
            IndexRunId::derive(&[b"repository-benchmark-incremental-run"]),
            GenerationId::derive(&[b"repository-benchmark-generation-2"]),
        ),
        _ => return Err(format!("stage {stage} does not index sources").into()),
    };
    let prior_status = engine.status()?;
    match (stage, prior_status.as_ref()) {
        ("initial", None) => {}
        ("initial", Some(_)) => {
            return Err(
                format!("{backend} initial stage requires an empty generation history").into(),
            );
        }
        ("incremental", Some(status))
            if status.manifest.generation
                == GenerationId::derive(&[b"repository-benchmark-generation-1"]) =>
        {
            // The separately traced initial stage must publish exactly one baseline generation.
        }
        ("incremental", _) => {
            return Err(format!(
                "{backend} incremental stage requires the benchmark baseline generation"
            )
            .into());
        }
        _ => return Err(format!("unsupported indexing stage: {stage}").into()),
    }
    let started = Instant::now();
    engine.index(sources, run_id, generation)?;
    let elapsed = started.elapsed();
    let status = engine
        .status()?
        .ok_or("staged indexing did not publish a generation")?;
    if !status.integrity.graph_root_matches || !status.integrity.references_valid {
        return Err(format!("{backend} {stage} graph failed integrity checks").into());
    }
    println!(
        "backend={backend} stage={stage} index_ms={} logical_input_bytes={logical_input_bytes} files={} nodes={} edges={} provenance={}",
        elapsed.as_millis(),
        status.files,
        status.nodes,
        status.edges,
        status.provenance
    );
    Ok(())
}

fn persisted_database_bytes(database_path: &Path) -> Result<u64, std::io::Error> {
    let mut total = fs::metadata(database_path)?.len();
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = database_path.as_os_str().to_os_string();
        sidecar.push(suffix);
        match fs::metadata(PathBuf::from(sidecar)) {
            Ok(metadata) => total = total.saturating_add(metadata.len()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(total)
}
