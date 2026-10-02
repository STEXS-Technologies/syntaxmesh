use syntaxmesh_analytics_parquet::export_fact_history;
use syntaxmesh_api_model::{
    ContextItemKind, ContextPack, ContextRequest, TEMPORAL_EXPORT_SCHEMA_VERSION,
    TemporalQueryMode, TemporalRecord,
};
use syntaxmesh_core::{
    AcceptanceTime, ChangeEventCorrelationCursor, ChangeSet, ChangeSetDelta, ChangeSetId,
    ChangeSetKind, ChangeSetMembership, ChangeSetMembershipKey, ChangeSetMembershipRemoval,
    ConsequenceDelta, ConsequenceDerivation, ConsequenceEdge, ConsequenceEdgeId, ConsequenceKind,
    ConsequenceRetraction, Edge, EdgeId, EvidenceClass, ExportKind, FactRef, FactVersionRef,
    FileId, FileVersion, GenerationHistoryEntry, GenerationId, GenerationManifest, GraphDelta,
    GraphDeltaWithConsequences, GraphDeltaWithLineage, GraphSnapshot, ImportKind, IndexRunId,
    LineageEndpoint, Node, NodeId, NodeKind, ObservationTime, Provenance, ProvenanceId,
    RelationKind, RepositoryId, SourceLocation, SourceSpan, WorktreeId,
};
use syntaxmesh_query::{
    ConsequenceTrace, ConsequenceTraceRequest, ContextSourceProvider, ContextTokenCounter, Query,
};
use syntaxmesh_store::{
    AcceptedGeneration, BackendIntegrityCheck, DurableRecordStore, FactHistoryCursor,
    FactHistoryEntry, FactHistoryVersion, FactVersionChangeCursor, FileGraphStore,
    GenerationChangeCursor, GraphStore, InMemoryGraphStore, MAX_FACT_VERSION_CHANGE_PAGE_SIZE,
    MAX_GENERATION_CHANGE_PAGE_SIZE, ObservedFactVersion, StoreError,
};
use syntaxmesh_store_sqlite::SqliteGraphStore;
use syntaxmesh_store_turso::TursoGraphStore;

#[path = "backend_conformance/historical_context.rs"]
mod historical_context;

#[path = "backend_conformance/context_adjacency.rs"]
mod context_adjacency;

#[path = "backend_conformance/node_pages.rs"]
mod node_pages;

#[path = "backend_conformance/file_pages.rs"]
mod file_pages;

#[path = "backend_conformance/path_index.rs"]
mod path_index;

fn open_migrated(path: impl AsRef<std::path::Path>) -> Result<SqliteGraphStore, StoreError> {
    SqliteGraphStore::migrate(path.as_ref())?;
    SqliteGraphStore::open(path)
}

fn open_turso(path: impl AsRef<std::path::Path>) -> Result<TursoGraphStore, StoreError> {
    TursoGraphStore::migrate(path.as_ref())
        .map_err(|error| StoreError::Backend(format!("migrate Turso fixture: {error}")))?;
    TursoGraphStore::open(path)
        .map_err(|error| StoreError::Backend(format!("open Turso fixture: {error}")))
}

struct ContextFixtureSource(Vec<u8>);

impl ContextSourceProvider for ContextFixtureSource {
    fn read_source(&self, file: &FileVersion) -> Result<Option<Vec<u8>>, String> {
        if file.normalized_path == "src/context.rs" {
            Ok(Some(self.0.clone()))
        } else {
            Ok(None)
        }
    }
}

struct ContextFixtureCounter;

impl ContextTokenCounter for ContextFixtureCounter {
    fn tokenizer_id(&self) -> &str {
        "test/serialized-byte-count-v1"
    }

    fn count_tokens(&self, serialized_pack: &str) -> Result<u64, String> {
        u64::try_from(serialized_pack.len()).map_err(|error| error.to_string())
    }
}

fn publish_context_fixture<S: GraphStore + ?Sized>(
    store: &mut S,
) -> Result<GenerationId, StoreError> {
    let content = b"pub fn caller() { helper(); }\npub fn helper() {}\n";
    let file_id = FileId::derive(&[b"context-conformance-file"]);
    let file = FileVersion {
        file_id,
        normalized_path: "src/context.rs".to_owned(),
        content_hash: *blake3::hash(content).as_bytes(),
        size_bytes: u64::try_from(content.len()).map_err(|error| {
            StoreError::Integrity(format!("convert context fixture size: {error}"))
        })?,
    };
    let caller_end = content
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or_else(|| StoreError::Integrity("context caller line is missing".to_owned()))?;
    let helper_start = caller_end.saturating_add(1);
    let source_location = |start: usize, end: usize| -> Result<SourceLocation, StoreError> {
        Ok(SourceLocation {
            file_id,
            content_hash: file.content_hash,
            span: SourceSpan {
                start_byte: u64::try_from(start).map_err(|error| {
                    StoreError::Integrity(format!("convert context span start: {error}"))
                })?,
                end_byte: u64::try_from(end).map_err(|error| {
                    StoreError::Integrity(format!("convert context span end: {error}"))
                })?,
            },
        })
    };
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"context-conformance-provenance"]),
        producer_namespace: "syntaxmesh.context.conformance".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: Some(source_location(0, caller_end)?),
    };
    let caller = Node {
        id: NodeId::derive(&[b"context-conformance-caller"]),
        kind: NodeKind::Function,
        name: "caller".to_owned(),
        owner_file: Some(file_id),
        source: Some(source_location(0, caller_end)?),
        provenance: provenance.id,
        extension_payload: None,
    };
    let helper = Node {
        id: NodeId::derive(&[b"context-conformance-helper"]),
        kind: NodeKind::Function,
        name: "helper".to_owned(),
        owner_file: Some(file_id),
        source: Some(source_location(
            helper_start,
            content.len().saturating_sub(1),
        )?),
        provenance: provenance.id,
        extension_payload: None,
    };
    store.apply_delta(GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"context-conformance-run"]),
        expected_base: None,
        next_generation: GenerationId::derive(&[b"context-conformance-generation"]),
        changed_files: vec![file],
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![caller.clone(), helper.clone()],
        upsert_edges: vec![Edge {
            id: EdgeId::derive(&[b"context-conformance-call-edge"]),
            source: caller.id,
            target: helper.id,
            relation: RelationKind::Calls,
            provenance: caller.provenance,
            extension_payload: None,
        }],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    Ok(GenerationId::derive(&[b"context-conformance-generation"]))
}

fn context_pack<S: GraphStore + ?Sized>(
    store: &S,
    generation: GenerationId,
) -> Result<ContextPack, StoreError> {
    let content = b"pub fn caller() { helper(); }\npub fn helper() {}\n";
    Query::new(store, generation)
        .context(
            &ContextRequest {
                query: "caller".to_owned(),
                seed_nodes: Vec::new(),
                token_budget: 4096,
                max_hops: 1,
                max_candidates: 16,
            },
            &ContextFixtureSource(content.to_vec()),
            &ContextFixtureCounter,
        )
        .map_err(|error| StoreError::Backend(format!("compile context fixture: {error}")))
}

fn collect_fact_history_pages<S: GraphStore + ?Sized>(
    store: &S,
    as_of: GenerationId,
) -> Result<Vec<FactHistoryEntry>, StoreError> {
    let mut entries = Vec::new();
    let mut cursor: Option<FactHistoryCursor> = None;
    loop {
        let page = store.fact_history_page(as_of, cursor, 2)?;
        entries.extend(page.items);
        let Some(next) = page.next_cursor else {
            return Ok(entries);
        };
        cursor = Some(next);
    }
}

fn publish_later_context_generation<S: GraphStore + ?Sized>(
    store: &mut S,
    base: GenerationId,
) -> Result<GenerationId, StoreError> {
    let mut snapshot = store.historical_snapshot(base)?;
    let mut node = snapshot
        .nodes
        .pop()
        .ok_or_else(|| StoreError::Integrity("context fixture has no node to update".to_owned()))?;
    node.name.push_str("_later");
    let generation = GenerationId::derive(&[b"context-conformance-later-generation"]);
    store.apply_delta(GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"context-conformance-later-run"]),
        expected_base: Some(base),
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: vec![node],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    Ok(generation)
}

#[test]
fn fact_history_pages_are_bounded_pinned_and_conform_across_backends() -> Result<(), StoreError> {
    let suffix = format!("{}-fact-history-pages", std::process::id());
    let file_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.snapshot"));
    let sqlite_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.sqlite"));
    let turso_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.turso"));
    let generation = GenerationId::derive(&[b"context-conformance-generation"]);
    let mut memory = InMemoryGraphStore::new();
    publish_context_fixture(&mut memory)?;
    let expected = collect_fact_history_pages(&memory, generation)?;
    if expected.len() != 5 {
        return Err(StoreError::Integrity(format!(
            "expected five canonical fact versions, got {}",
            expected.len()
        )));
    }

    let mut file = FileGraphStore::open(&file_path)?;
    publish_context_fixture(&mut file)?;
    let mut sqlite = open_migrated(&sqlite_path)?;
    publish_context_fixture(&mut sqlite)?;
    let mut turso = open_turso(&turso_path)?;
    publish_context_fixture(&mut turso)?;

    for store in [
        &mut memory as &mut dyn GraphStore,
        &mut file,
        &mut sqlite,
        &mut turso,
    ] {
        let first = store.fact_history_page(generation, None, 2)?;
        if first.items.len() != 2 || first.next_cursor.is_none() {
            return Err(StoreError::Integrity(
                "fact-history page was not bounded or did not offer continuation".to_owned(),
            ));
        }
        let later = publish_later_context_generation(store, generation)?;
        let continued = collect_fact_history_pages(store, generation)?;
        if continued != expected {
            return Err(StoreError::Integrity(
                "pinned fact-history continuation changed after a later publication".to_owned(),
            ));
        }
        let cursor = first
            .next_cursor
            .ok_or_else(|| StoreError::Integrity("fact-history cursor disappeared".to_owned()))?;
        let mismatch = store.fact_history_page(later, Some(cursor), 2);
        if mismatch.is_ok() {
            return Err(StoreError::Integrity(format!(
                "fact-history cursor was not rejected for another snapshot: {mismatch:?}"
            )));
        }
    }

    // The exporter consumes only the shared GraphStore history port. Verify
    // the complete published artifact (manifest, checksums, and Parquet bytes)
    // is identical regardless of the canonical backend implementation.
    let export_root = tempfile::tempdir().map_err(|error| {
        StoreError::Backend(format!("create Parquet conformance directory: {error}"))
    })?;
    let export_directories =
        ["memory", "file", "sqlite", "turso"].map(|name| export_root.path().join(name));
    for (store, destination) in [&memory as &dyn GraphStore, &file, &sqlite, &turso]
        .into_iter()
        .zip(export_directories.iter())
    {
        let exported = export_fact_history(store, generation, destination, 2)
            .map_err(|error| StoreError::Backend(format!("export conformance history: {error}")))?;
        if !exported.complete || exported.partition_count == 0 {
            return Err(StoreError::Integrity(format!(
                "fact-history Parquet export did not complete: {exported:?}"
            )));
        }
    }
    let mut reference_entries = std::fs::read_dir(&export_directories[0])
        .map_err(|error| StoreError::Backend(format!("read reference export: {error}")))?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|error| StoreError::Backend(format!("read export entry: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    reference_entries.sort();
    for destination in export_directories.iter().skip(1) {
        let mut backend_entries = std::fs::read_dir(destination)
            .map_err(|error| StoreError::Backend(format!("read backend export: {error}")))?
            .map(|entry| {
                entry
                    .map(|entry| entry.file_name())
                    .map_err(|error| StoreError::Backend(format!("read export entry: {error}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        backend_entries.sort();
        if backend_entries != reference_entries {
            return Err(StoreError::Integrity(
                "Parquet artifact file sets differ across graph-store backends".to_owned(),
            ));
        }
        for name in &reference_entries {
            let expected_bytes =
                std::fs::read(export_directories[0].join(name)).map_err(|error| {
                    StoreError::Backend(format!("read reference Parquet artifact: {error}"))
                })?;
            let actual_bytes = std::fs::read(destination.join(name)).map_err(|error| {
                StoreError::Backend(format!("read backend Parquet artifact: {error}"))
            })?;
            if actual_bytes != expected_bytes {
                return Err(StoreError::Integrity(format!(
                    "Parquet artifact {:?} differs across graph-store backends",
                    name
                )));
            }
        }
    }
    drop(memory);
    drop(file);
    drop(sqlite);
    drop(turso);
    for path in [&file_path, &sqlite_path, &turso_path] {
        std::fs::remove_file(path).map_err(|error| {
            StoreError::Backend(format!("remove fact-history fixture: {error}"))
        })?;
    }
    Ok(())
}

#[test]
fn context_packs_are_equivalent_across_backends_and_durable_restarts() -> Result<(), StoreError> {
    let suffix = format!("{}-context-pack", std::process::id());
    let file_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.snapshot"));
    let sqlite_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.sqlite"));
    let turso_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.turso"));
    let generation = GenerationId::derive(&[b"context-conformance-generation"]);

    let mut memory = InMemoryGraphStore::new();
    if publish_context_fixture(&mut memory)? != generation {
        return Err(StoreError::Integrity(
            "unexpected context generation".to_owned(),
        ));
    }
    let expected = context_pack(&memory, generation)?;
    if expected.token_count > expected.token_budget
        || !expected
            .items
            .iter()
            .any(|item| item.kind == ContextItemKind::SourceEvidence && item.line_start == Some(1))
    {
        return Err(StoreError::Integrity(
            "context fixture omitted bounded source evidence".to_owned(),
        ));
    }

    let mut file = FileGraphStore::open(&file_path)?;
    publish_context_fixture(&mut file)?;
    if context_pack(&file, generation)? != expected {
        return Err(StoreError::Integrity(
            "File context differs from InMemory".to_owned(),
        ));
    }
    drop(file);
    let reopened_file = FileGraphStore::open(&file_path)?;
    if context_pack(&reopened_file, generation)? != expected {
        return Err(StoreError::Integrity(
            "restarted File context differs from InMemory".to_owned(),
        ));
    }
    drop(reopened_file);

    let mut sqlite = open_migrated(&sqlite_path)?;
    publish_context_fixture(&mut sqlite)?;
    if context_pack(&sqlite, generation)? != expected {
        return Err(StoreError::Integrity(
            "SQLite context differs from InMemory".to_owned(),
        ));
    }
    drop(sqlite);
    let reopened_sqlite = SqliteGraphStore::open(&sqlite_path)?;
    if context_pack(&reopened_sqlite, generation)? != expected {
        return Err(StoreError::Integrity(
            "restarted SQLite context differs from InMemory".to_owned(),
        ));
    }
    drop(reopened_sqlite);

    let mut turso = open_turso(&turso_path)?;
    publish_context_fixture(&mut turso)?;
    if context_pack(&turso, generation)? != expected {
        return Err(StoreError::Integrity(
            "Turso context differs from InMemory".to_owned(),
        ));
    }
    drop(turso);
    let reopened_turso = open_turso(&turso_path)?;
    if context_pack(&reopened_turso, generation)? != expected {
        return Err(StoreError::Integrity(
            "restarted Turso context differs from InMemory".to_owned(),
        ));
    }
    drop(reopened_turso);

    for path in [&file_path, &sqlite_path, &turso_path] {
        std::fs::remove_file(path)
            .map_err(|error| StoreError::Backend(format!("remove context fixture: {error}")))?;
    }
    Ok(())
}

fn check_durable_record_pages<S: DurableRecordStore>(store: &mut S) -> Result<(), StoreError> {
    let prefix = "syntaxmesh.test.page/";
    for suffix in ["a", "b", "c", "d", "e"] {
        let key = format!("{prefix}{suffix}");
        store.compare_exchange_record(&key, None, suffix.as_bytes())?;
    }
    let first = store.records_with_prefix_page(prefix, None, 2)?;
    let cursor = first.next_cursor.as_deref().ok_or_else(|| {
        StoreError::Integrity("durable record first page is missing its cursor".to_owned())
    })?;
    if first
        .records
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>()
        != ["syntaxmesh.test.page/a", "syntaxmesh.test.page/b"]
    {
        return Err(StoreError::Integrity(
            "durable record first page is not ordered or bounded".to_owned(),
        ));
    }
    let second = store.records_with_prefix_page(prefix, Some(cursor), 2)?;
    if second
        .records
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>()
        != ["syntaxmesh.test.page/c", "syntaxmesh.test.page/d"]
    {
        return Err(StoreError::Integrity(
            "durable record second page skipped or repeated a key".to_owned(),
        ));
    }
    let third = store.records_with_prefix_page(prefix, second.next_cursor.as_deref(), 2)?;
    if third.records.len() != 1
        || third.records.first().map(|(key, _)| key.as_str()) != Some("syntaxmesh.test.page/e")
        || third.next_cursor.is_some()
    {
        return Err(StoreError::Integrity(
            "durable record final page has an invalid cursor".to_owned(),
        ));
    }
    let empty = store.records_with_prefix_page(prefix, None, 0)?;
    if !empty.records.is_empty() || empty.next_cursor.is_some() {
        return Err(StoreError::Integrity(
            "zero-sized durable record page is not empty".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn durable_record_pages_are_ordered_bounded_and_conform_across_backends() -> Result<(), StoreError>
{
    let suffix = format!("{}-durable-record-page", std::process::id());
    let file_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.snapshot"));
    let sqlite_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.sqlite"));
    let turso_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.turso"));

    check_durable_record_pages(&mut InMemoryGraphStore::new())?;
    check_durable_record_pages(&mut FileGraphStore::open(&file_path)?)?;
    check_durable_record_pages(&mut open_migrated(&sqlite_path)?)?;
    check_durable_record_pages(&mut open_turso(&turso_path)?)?;

    for path in [&file_path, &sqlite_path, &turso_path] {
        std::fs::remove_file(path)
            .map_err(|error| StoreError::Backend(format!("remove record-page fixture: {error}")))?;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ExplicitLineageExpectation {
    change_set: ChangeSetId,
    assignment_generation: GenerationId,
    removal_generation: GenerationId,
    retained_event: syntaxmesh_core::ChangeEventId,
}

#[test]
fn explicit_change_set_history_conforms_across_all_backends_and_restart() -> Result<(), StoreError>
{
    let suffix = format!("{}-explicit-lineage", std::process::id());
    let file_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.snapshot"));
    let sqlite_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.sqlite"));
    let turso_path = std::env::temp_dir().join(format!("syntaxmesh-{suffix}.turso"));

    let mut memory = InMemoryGraphStore::new();
    let expected = exercise_explicit_lineage(&mut memory)?;
    let mut file = FileGraphStore::open(&file_path)?;
    if exercise_explicit_lineage(&mut file)? != expected {
        return Err(StoreError::Integrity(
            "File ChangeSet behavior differs from InMemory".to_owned(),
        ));
    }
    drop(file);
    let mut sqlite = open_migrated(&sqlite_path)?;
    if exercise_explicit_lineage(&mut sqlite)? != expected {
        return Err(StoreError::Integrity(
            "SQLite ChangeSet behavior differs from InMemory".to_owned(),
        ));
    }
    drop(sqlite);
    let mut turso = open_turso(&turso_path)?;
    if exercise_explicit_lineage(&mut turso)? != expected {
        return Err(StoreError::Integrity(
            "Turso ChangeSet behavior differs from InMemory".to_owned(),
        ));
    }
    drop(turso);

    for path in [&file_path, &sqlite_path, &turso_path] {
        let store = match path.extension().and_then(std::ffi::OsStr::to_str) {
            Some("snapshot") => ReopenedStore::File(FileGraphStore::open(path)?),
            Some("sqlite") => ReopenedStore::Sqlite(SqliteGraphStore::open(path)?),
            Some("turso") => ReopenedStore::Turso(open_turso(path)?),
            _ => {
                return Err(StoreError::Integrity(
                    "unknown fixture extension".to_owned(),
                ));
            }
        };
        let query = match &store {
            ReopenedStore::File(store) => verify_lineage_restart(store, expected),
            ReopenedStore::Sqlite(store) => verify_lineage_restart(store, expected),
            ReopenedStore::Turso(store) => verify_lineage_restart(store, expected),
        };
        query?;
    }
    for path in [&file_path, &sqlite_path, &turso_path] {
        std::fs::remove_file(path)
            .map_err(|error| StoreError::Backend(format!("remove ChangeSet fixture: {error}")))?;
    }
    Ok(())
}

enum ReopenedStore {
    File(FileGraphStore),
    Sqlite(SqliteGraphStore),
    Turso(TursoGraphStore),
}

fn verify_lineage_restart<S: GraphStore>(
    store: &S,
    expected: ExplicitLineageExpectation,
) -> Result<(), StoreError> {
    let historical = store.events_for_change_set(
        expected.change_set,
        expected.assignment_generation,
        None,
        10,
    )?;
    let current =
        store.events_for_change_set(expected.change_set, expected.removal_generation, None, 10)?;
    let historical_declaration =
        store.change_set_at(expected.change_set, expected.assignment_generation)?;
    let declaration = store.change_set_at(expected.change_set, expected.removal_generation)?;
    if historical.items.len() != 2
        || current.items.len() != 1
        || current.items.first().map(|item| item.event.id) != Some(expected.retained_event)
        || historical_declaration.as_ref().is_none_or(|version| {
            version.change_set.title.as_deref() != Some("cross-backend set")
                || version.valid_from != expected.assignment_generation
                || version.valid_until != Some(expected.removal_generation)
        })
        || declaration.as_ref().is_none_or(|version| {
            version.change_set.title.as_deref() != Some("updated cross-backend set")
                || version.valid_from != expected.removal_generation
                || version.valid_until.is_some()
        })
        || store.generation_lineage_history()?.len() != 4
    {
        return Err(StoreError::Integrity(
            "ChangeSet history did not survive backend restart".to_owned(),
        ));
    }
    Ok(())
}

fn exercise_explicit_lineage<S: GraphStore>(
    store: &mut S,
) -> Result<ExplicitLineageExpectation, StoreError> {
    let repository = RepositoryId::derive(&[b"explicit-lineage-repository"]);
    let worktree = WorktreeId::derive(&[b"explicit-lineage-worktree"]);
    let generations = [
        GenerationId::derive(&[b"explicit-lineage-generation-1"]),
        GenerationId::derive(&[b"explicit-lineage-generation-2"]),
        GenerationId::derive(&[b"explicit-lineage-generation-3"]),
        GenerationId::derive(&[b"explicit-lineage-generation-4"]),
        GenerationId::derive(&[b"explicit-lineage-generation-5"]),
    ];
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"explicit-lineage-provenance"]),
        producer_namespace: "conformance.change-set".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let empty_delta = |base, next, run: &'static [u8]| GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[run]),
        expected_base: base,
        next_generation: next,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    let mut first_delta = empty_delta(None, generations[0], b"explicit-lineage-run-1");
    first_delta.upsert_provenance.push(provenance.clone());
    store.apply_delta(first_delta)?;
    store.apply_delta(empty_delta(
        Some(generations[0]),
        generations[1],
        b"explicit-lineage-run-2",
    ))?;
    let first_event = store
        .change_event(generations[0])?
        .ok_or_else(|| StoreError::Integrity("first event is absent".to_owned()))?;
    let second_event = store
        .change_event(generations[1])?
        .ok_or_else(|| StoreError::Integrity("second event is absent".to_owned()))?;
    let change_set = ChangeSetId::derive(&[b"explicit-lineage-set"]);
    store.apply_delta_with_lineage(
        GraphDeltaWithLineage {
            graph: empty_delta(
                Some(generations[1]),
                generations[2],
                b"explicit-lineage-run-3",
            ),
            lineage: ChangeSetDelta {
                upsert_sets: vec![ChangeSet {
                    id: change_set,
                    kind: ChangeSetKind::ManualGroup,
                    title: Some("cross-backend set".to_owned()),
                    originating_intent: None,
                    parent_changes: Vec::new(),
                    git_commits: Vec::new(),
                    pull_requests: Vec::new(),
                    issues: Vec::new(),
                    adrs: Vec::new(),
                    repositories: vec![repository],
                    first_generation: generations[0],
                    last_generation: Some(generations[2]),
                    provenance: provenance.id,
                }],
                assign_events: vec![
                    ChangeSetMembership {
                        change_set,
                        event: first_event.id,
                        provenance: provenance.id,
                    },
                    ChangeSetMembership {
                        change_set,
                        event: second_event.id,
                        provenance: provenance.id,
                    },
                ],
                unassign_events: Vec::new(),
            },
        },
        None,
    )?;
    let prior = store.events_for_change_set(change_set, generations[1], None, 10)?;
    let prior_declaration = store.change_set_at(change_set, generations[1])?;
    let declaration = store.change_set_at(change_set, generations[2])?;
    let first_page = store.events_for_change_set(change_set, generations[2], None, 1)?;
    let cursor = first_page
        .next_cursor
        .ok_or_else(|| StoreError::Integrity("ChangeSet page omitted its cursor".to_owned()))?;
    let continued = store.events_for_change_set(change_set, generations[2], Some(cursor), 1)?;
    if !prior.items.is_empty()
        || prior_declaration.is_some()
        || declaration.as_ref().is_none_or(|version| {
            version.change_set.id != change_set
                || version.valid_from != generations[2]
                || version.valid_until.is_some()
        })
        || first_page.items.first().map(|item| item.event.id) != Some(first_event.id)
        || continued.items.first().map(|item| item.event.id) != Some(second_event.id)
    {
        return Err(StoreError::Integrity(
            "ChangeSet query validity or pagination differs".to_owned(),
        ));
    }
    store.apply_delta_with_lineage(
        GraphDeltaWithLineage {
            graph: empty_delta(
                Some(generations[2]),
                generations[3],
                b"explicit-lineage-run-4",
            ),
            lineage: ChangeSetDelta {
                upsert_sets: vec![ChangeSet {
                    id: change_set,
                    kind: ChangeSetKind::ManualGroup,
                    title: Some("updated cross-backend set".to_owned()),
                    originating_intent: None,
                    parent_changes: Vec::new(),
                    git_commits: Vec::new(),
                    pull_requests: Vec::new(),
                    issues: Vec::new(),
                    adrs: Vec::new(),
                    repositories: vec![repository],
                    first_generation: generations[0],
                    last_generation: Some(generations[3]),
                    provenance: provenance.id,
                }],
                unassign_events: vec![ChangeSetMembershipRemoval {
                    key: ChangeSetMembershipKey {
                        change_set,
                        event: first_event.id,
                    },
                    provenance: provenance.id,
                }],
                ..ChangeSetDelta::default()
            },
        },
        None,
    )?;
    assert_rejected_lineage_is_atomic(
        store,
        repository,
        worktree,
        GraphDeltaWithLineage {
            graph: empty_delta(
                Some(generations[3]),
                generations[4],
                b"explicit-lineage-run-5",
            ),
            lineage: ChangeSetDelta {
                assign_events: vec![ChangeSetMembership {
                    change_set,
                    event: syntaxmesh_core::ChangeEventId::derive(&[b"unknown-lineage-event"]),
                    provenance: provenance.id,
                }],
                ..ChangeSetDelta::default()
            },
        },
    )?;

    let unknown_change_set = ChangeSetId::derive(&[b"unknown-lineage-set"]);
    assert_rejected_lineage_is_atomic(
        store,
        repository,
        worktree,
        GraphDeltaWithLineage {
            graph: empty_delta(
                Some(generations[3]),
                generations[4],
                b"explicit-lineage-run-unknown-set",
            ),
            lineage: ChangeSetDelta {
                assign_events: vec![ChangeSetMembership {
                    change_set: unknown_change_set,
                    event: first_event.id,
                    provenance: provenance.id,
                }],
                ..ChangeSetDelta::default()
            },
        },
    )?;

    let unknown_provenance = ProvenanceId::derive(&[b"unknown-lineage-provenance"]);
    assert_rejected_lineage_is_atomic(
        store,
        repository,
        worktree,
        GraphDeltaWithLineage {
            graph: empty_delta(
                Some(generations[3]),
                generations[4],
                b"explicit-lineage-run-unknown-provenance",
            ),
            lineage: ChangeSetDelta {
                unassign_events: vec![ChangeSetMembershipRemoval {
                    key: ChangeSetMembershipKey {
                        change_set,
                        event: second_event.id,
                    },
                    provenance: unknown_provenance,
                }],
                ..ChangeSetDelta::default()
            },
        },
    )?;

    assert_rejected_lineage_is_atomic(
        store,
        repository,
        worktree,
        GraphDeltaWithLineage {
            graph: empty_delta(
                Some(generations[3]),
                generations[4],
                b"explicit-lineage-run-conflicting-membership",
            ),
            lineage: ChangeSetDelta {
                assign_events: vec![ChangeSetMembership {
                    change_set,
                    event: second_event.id,
                    provenance: provenance.id,
                }],
                unassign_events: vec![ChangeSetMembershipRemoval {
                    key: ChangeSetMembershipKey {
                        change_set,
                        event: second_event.id,
                    },
                    provenance: provenance.id,
                }],
                ..ChangeSetDelta::default()
            },
        },
    )?;

    Ok(ExplicitLineageExpectation {
        change_set,
        assignment_generation: generations[2],
        removal_generation: generations[3],
        retained_event: second_event.id,
    })
}

fn assert_rejected_lineage_is_atomic<S: GraphStore>(
    store: &mut S,
    repository: RepositoryId,
    worktree: WorktreeId,
    request: GraphDeltaWithLineage,
) -> Result<(), StoreError> {
    let expected_generation = request.graph.expected_base.ok_or_else(|| {
        StoreError::Integrity("invalid-lineage fixture omitted its expected base".to_owned())
    })?;
    let history_before = store.generation_lineage_history()?;
    let result = store.apply_delta_with_lineage(request, None);
    let latest_generation = store
        .current_generation(repository, worktree)?
        .map(|manifest| manifest.generation);
    if !matches!(result, Err(StoreError::InvalidDelta(_)))
        || latest_generation != Some(expected_generation)
        || store.generation_lineage_history()? != history_before
    {
        return Err(StoreError::Integrity(
            "invalid ChangeSet lineage changed current or historical state".to_owned(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogicalSnapshot {
    manifest: GenerationManifest,
    files: Vec<FileVersion>,
    provenance: Vec<Provenance>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    history: Vec<GenerationHistoryEntry>,
    accepted: Vec<AcceptedGeneration>,
    fact_histories: Vec<(FactRef, Vec<FactHistoryVersion>)>,
    observed_facts: Vec<ObservedFactVersion>,
}

struct FactLineageScenario {
    generation: GenerationId,
    lineage: Vec<TemporalRecord>,
    events: Vec<TemporalRecord>,
    correlations: Vec<TemporalRecord>,
}

#[test]
fn fact_lineage_uses_retained_fact_versions_across_backends_and_restart() -> Result<(), StoreError>
{
    let suffix = std::process::id().to_string();
    let base_path = std::env::temp_dir().join(format!("syntaxmesh-lineage-{suffix}"));
    let file_path = base_path.with_extension("snapshot");
    let turso_path = base_path.with_extension("db");
    let sqlite_path = base_path.with_extension("sqlite");

    let mut memory = InMemoryGraphStore::new();
    let FactLineageScenario {
        generation,
        lineage: expected,
        events: expected_events,
        correlations: expected_correlations,
    } = publish_fact_lineage(&mut memory)?;
    assert_single_supersedes(&expected)?;

    let mut file = FileGraphStore::open(&file_path)?;
    let FactLineageScenario {
        generation: file_generation,
        lineage: file_lineage,
        events: file_events,
        correlations: file_correlations,
    } = publish_fact_lineage(&mut file)?;
    drop(file);
    let reopened_file = FileGraphStore::open(&file_path)?;
    let file_restart = Query::new(&reopened_file, file_generation)
        .fact_lineage(node_fact_ref())
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let file_event_restart = Query::new(&reopened_file, file_generation)
        .change_events_for_fact(node_fact_ref(), None, 1)
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let file_direct_event = Query::new(&reopened_file, file_generation)
        .change_event()
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let file_correlation_restart = Query::new(
        &reopened_file,
        GenerationId::derive(&[b"lineage-generation-one"]),
    )
    .change_event_correlations(node_fact_ref(), None, 10)
    .map_err(|error| StoreError::Backend(error.to_string()))?;

    let mut turso = open_turso(&turso_path)?;
    let FactLineageScenario {
        generation: turso_generation,
        lineage: turso_lineage,
        events: turso_events,
        correlations: turso_correlations,
    } = publish_fact_lineage(&mut turso)?;
    drop(turso);
    let reopened_turso = open_turso(&turso_path)?;
    let turso_restart = Query::new(&reopened_turso, turso_generation)
        .fact_lineage(node_fact_ref())
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let turso_event_restart = Query::new(&reopened_turso, turso_generation)
        .change_events_for_fact(node_fact_ref(), None, 1)
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let turso_direct_event = Query::new(&reopened_turso, turso_generation)
        .change_event()
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let turso_correlation_restart = Query::new(
        &reopened_turso,
        GenerationId::derive(&[b"lineage-generation-one"]),
    )
    .change_event_correlations(node_fact_ref(), None, 10)
    .map_err(|error| StoreError::Backend(error.to_string()))?;

    let mut sqlite = open_migrated(&sqlite_path)?;
    let FactLineageScenario {
        generation: sqlite_generation,
        lineage: sqlite_lineage,
        events: sqlite_events,
        correlations: sqlite_correlations,
    } = publish_fact_lineage(&mut sqlite)?;
    drop(sqlite);
    let reopened_sqlite = open_migrated(&sqlite_path)?;
    let sqlite_restart = Query::new(&reopened_sqlite, sqlite_generation)
        .fact_lineage(node_fact_ref())
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let sqlite_event_restart = Query::new(&reopened_sqlite, sqlite_generation)
        .change_events_for_fact(node_fact_ref(), None, 1)
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let sqlite_direct_event = Query::new(&reopened_sqlite, sqlite_generation)
        .change_event()
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let sqlite_correlation_restart = Query::new(
        &reopened_sqlite,
        GenerationId::derive(&[b"lineage-generation-one"]),
    )
    .change_event_correlations(node_fact_ref(), None, 10)
    .map_err(|error| StoreError::Backend(error.to_string()))?;

    drop(reopened_file);
    drop(reopened_turso);
    drop(reopened_sqlite);
    for path in [&file_path, &turso_path, &sqlite_path] {
        std::fs::remove_file(path).map_err(|error| {
            StoreError::Backend(format!("remove lineage conformance fixture: {error}"))
        })?;
    }

    let expected_events_page = expected_events.iter().take(2).cloned().collect::<Vec<_>>();
    let expected_direct_event = expected_events
        .iter()
        .find(|record| {
            matches!(record, TemporalRecord::ChangeEvent { event, .. } if event.generation_after == generation)
        })
        .cloned();
    if generation != file_generation
        || generation != turso_generation
        || generation != sqlite_generation
        || expected != file_lineage
        || expected != turso_lineage
        || expected != sqlite_lineage
        || expected != file_restart
        || expected != turso_restart
        || expected != sqlite_restart
        || expected_events != file_events
        || expected_events != turso_events
        || expected_events != sqlite_events
        || expected_correlations != file_correlations
        || expected_correlations != turso_correlations
        || expected_correlations != sqlite_correlations
        || file_event_restart != expected_events_page
        || turso_event_restart != expected_events_page
        || sqlite_event_restart != expected_events_page
        || file_direct_event != expected_direct_event
        || turso_direct_event != expected_direct_event
        || sqlite_direct_event != expected_direct_event
        || file_correlation_restart != expected_correlations
        || turso_correlation_restart != expected_correlations
        || sqlite_correlation_restart != expected_correlations
    {
        return Err(StoreError::Integrity(
            "fact-version lineage differs across stores or restart".to_owned(),
        ));
    }
    Ok(())
}

fn publish_fact_lineage<S: GraphStore>(store: &mut S) -> Result<FactLineageScenario, StoreError> {
    let repository = repository();
    let worktree = worktree();
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"lineage-provenance"]),
        producer_namespace: "syntaxmesh.lineage.fixture".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let original = Node {
        id: NodeId::derive(&[b"lineage-fact"]),
        kind: NodeKind::Function,
        name: "lineage-before".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let first_generation = GenerationId::derive(&[b"lineage-generation-one"]);
    let first_manifest = store.apply_delta_at(
        GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"lineage-run-one"]),
            expected_base: None,
            next_generation: first_generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: vec![provenance],
            upsert_nodes: vec![original.clone()],
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        },
        AcceptanceTime(10),
    )?;
    let updated_generation = GenerationId::derive(&[b"lineage-generation-two"]);
    store.apply_delta_at(
        GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"lineage-run-two"]),
            expected_base: Some(first_manifest.generation),
            next_generation: updated_generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: vec![Node {
                name: "lineage-after".to_owned(),
                ..original
            }],
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        },
        AcceptanceTime(20),
    )?;
    let no_op_generation = GenerationId::derive(&[b"lineage-generation-no-op"]);
    store.apply_delta_at(
        GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"lineage-run-no-op"]),
            expected_base: Some(updated_generation),
            next_generation: no_op_generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        },
        AcceptanceTime(30),
    )?;
    if !matches!(store.change_event(no_op_generation)?, Some(event) if event.changed_facts.is_empty())
    {
        return Err(StoreError::Integrity(
            "no-op accepted generation is missing its atomic change event".to_owned(),
        ));
    }
    let lineage = Query::new(store, updated_generation)
        .fact_lineage(node_fact_ref())
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let first_page = Query::new(store, updated_generation)
        .change_events_for_fact(node_fact_ref(), None, 1)
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let cursor = first_page.iter().find_map(|record| match record {
        TemporalRecord::ChangeEventFooter {
            next_cursor: Some(cursor),
            ..
        } => Some(*cursor),
        TemporalRecord::ChangeEventFooter {
            next_cursor: None, ..
        } => None,
        TemporalRecord::AcceptedGeneration { .. }
        | TemporalRecord::HistoricalNeighborhoodNode { .. }
        | TemporalRecord::HistoricalNeighborhoodEdge { .. }
        | TemporalRecord::HistoricalNeighborhoodFooter { .. }
        | TemporalRecord::AcceptanceTimelineFooter { .. }
        | TemporalRecord::NodeVersion { .. }
        | TemporalRecord::Change { .. }
        | TemporalRecord::FactVersion { .. }
        | TemporalRecord::FactSupersedes { .. }
        | TemporalRecord::ChangeEvent { .. }
        | TemporalRecord::ChangeEventCorrelation { .. }
        | TemporalRecord::ChangeEventCorrelationFooter { .. }
        | TemporalRecord::ChangeSetEvent { .. }
        | TemporalRecord::ChangeSetEventsFooter { .. }
        | TemporalRecord::ChangeSetVersion { .. }
        | TemporalRecord::ConsequenceEdge { .. }
        | TemporalRecord::ConsequenceNeighborhoodFooter { .. }
        | TemporalRecord::ConsequenceTraceHeader { .. }
        | TemporalRecord::ConsequenceTraceState { .. }
        | TemporalRecord::ConsequenceTraceHop { .. }
        | TemporalRecord::ConsequenceTraceFooter { .. }
        | TemporalRecord::ObservedFact { .. }
        | TemporalRecord::ObservedTimelineFooter { .. }
        | TemporalRecord::HistoricalNeighbor { .. }
        | TemporalRecord::HistoricalNeighborFooter { .. } => None,
    });
    let second_page = Query::new(store, updated_generation)
        .change_events_for_fact(node_fact_ref(), cursor, 1)
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let correlations = Query::new(store, first_generation)
        .change_event_correlations(node_fact_ref(), None, 10)
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    let source_event = store
        .change_event(first_generation)?
        .ok_or_else(|| StoreError::Integrity("source change event is missing".to_owned()))?;
    let cursor_correlations = Query::new(store, first_generation)
        .change_event_correlations(
            node_fact_ref(),
            Some(ChangeEventCorrelationCursor {
                source_event: source_event.id,
                after_generation: first_generation,
            }),
            10,
        )
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    if cursor_correlations != correlations {
        return Err(StoreError::Integrity(
            "correlation cursor page differs from the initial page".to_owned(),
        ));
    }
    let mut events = first_page;
    events.extend(second_page);
    if cursor.is_none()
        || !matches!(events.first(), Some(TemporalRecord::ChangeEvent { event, .. }) if event.generation_after == first_generation && event.changed_facts.iter().any(|changed| changed.fact == node_fact_ref() && changed.kind == syntaxmesh_core::FactChangeKind::Added))
        || !matches!(events.get(2), Some(TemporalRecord::ChangeEvent { event, .. }) if event.generation_after == updated_generation && event.changed_facts.iter().any(|changed| changed.fact == node_fact_ref() && changed.kind == syntaxmesh_core::FactChangeKind::Updated))
        || !matches!(
            events.get(3),
            Some(TemporalRecord::ChangeEventFooter {
                has_more: false,
                next_cursor: None,
                ..
            })
        )
    {
        return Err(StoreError::Integrity(format!(
            "fact-linked change-event pagination or classification is incorrect: {events:?}"
        )));
    }
    Ok(FactLineageScenario {
        generation: updated_generation,
        lineage,
        events,
        correlations,
    })
}

fn node_fact_ref() -> FactRef {
    FactRef::Node(NodeId::derive(&[b"lineage-fact"]))
}

fn assert_single_supersedes(lineage: &[TemporalRecord]) -> Result<(), StoreError> {
    if !matches!(
        lineage,
        [TemporalRecord::FactSupersedes {
            schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
            query_mode: TemporalQueryMode::HistoricalConclusion,
            prior,
            current,
            accepted_at: Some(AcceptanceTime(20)),
        }]
            if prior.fact == node_fact_ref()
                && current.fact == node_fact_ref()
                && prior.valid_from == GenerationId::derive(&[b"lineage-generation-one"])
                && current.valid_from == GenerationId::derive(&[b"lineage-generation-two"])
    ) {
        return Err(StoreError::Integrity(
            "same-identity lineage did not produce its supersedes link".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn implicit_edge_cascade_is_indexed_and_survives_durable_restart() -> Result<(), StoreError> {
    let suffix = std::process::id().to_string();
    let base = std::env::temp_dir().join(format!("syntaxmesh-event-cascade-{suffix}"));
    let file_path = base.with_extension("snapshot");
    let sqlite_path = base.with_extension("sqlite");
    let turso_path = base.with_extension("db");

    let expected = exercise_cascade_events(&mut InMemoryGraphStore::new())?;
    let mut file = FileGraphStore::open(&file_path)?;
    let file_events = exercise_cascade_events(&mut file)?;
    drop(file);
    let reopened_file = FileGraphStore::open(&file_path)?;
    let file_restart = read_edge_change_events(&reopened_file)?;

    let mut sqlite = open_migrated(&sqlite_path)?;
    let sqlite_events = exercise_cascade_events(&mut sqlite)?;
    drop(sqlite);
    let reopened_sqlite = open_migrated(&sqlite_path)?;
    let sqlite_restart = read_edge_change_events(&reopened_sqlite)?;

    let mut turso = open_turso(&turso_path)?;
    let turso_events = exercise_cascade_events(&mut turso)?;
    drop(turso);
    let reopened_turso = open_turso(&turso_path)?;
    let turso_restart = read_edge_change_events(&reopened_turso)?;

    drop(reopened_file);
    drop(reopened_sqlite);
    drop(reopened_turso);
    for path in [&file_path, &sqlite_path, &turso_path] {
        std::fs::remove_file(path)
            .map_err(|error| StoreError::Backend(format!("remove event fixture: {error}")))?;
    }
    if expected != file_events
        || expected != sqlite_events
        || expected != turso_events
        || expected != file_restart
        || expected != sqlite_restart
        || expected != turso_restart
    {
        return Err(StoreError::Integrity(
            "implicit-edge change-event projection differs across stores or restart".to_owned(),
        ));
    }
    Ok(())
}

fn exercise_cascade_events<S: GraphStore>(
    store: &mut S,
) -> Result<Vec<TemporalRecord>, StoreError> {
    let first = store.apply_delta(first_delta())?;
    let removed_generation = GenerationId::derive(&[b"event-cascade-generation-two"]);
    store.apply_delta(GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"event-cascade-run-two"]),
        expected_base: Some(first.generation),
        next_generation: removed_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: vec![NodeId::derive(&[b"conformance-node-first"])],
        remove_edges: Vec::new(),
    })?;
    let records = read_edge_change_events(store)?;
    if !matches!(records.as_slice(), [
        TemporalRecord::ChangeEvent { event: first_event, .. },
        TemporalRecord::ChangeEvent { event: second_event, .. },
        TemporalRecord::ChangeEventFooter { returned: 2, has_more: false, .. }
    ] if first_event.changed_facts.iter().any(|changed| changed.fact == FactRef::Edge(EdgeId::derive(&[b"conformance-edge"])) && changed.kind == syntaxmesh_core::FactChangeKind::Added)
        && second_event.generation_after == removed_generation
        && second_event.changed_facts.iter().any(|changed| changed.fact == FactRef::Edge(EdgeId::derive(&[b"conformance-edge"])) && changed.kind == syntaxmesh_core::FactChangeKind::Removed))
    {
        return Err(StoreError::Integrity(
            "implicit incident-edge deletion was not represented in change lineage".to_owned(),
        ));
    }
    Ok(records)
}

fn read_edge_change_events<S: GraphStore>(store: &S) -> Result<Vec<TemporalRecord>, StoreError> {
    Query::new(
        store,
        GenerationId::derive(&[b"event-cascade-generation-two"]),
    )
    .change_events_for_fact(
        FactRef::Edge(EdgeId::derive(&[b"conformance-edge"])),
        None,
        10,
    )
    .map_err(|error| StoreError::Backend(error.to_string()))
}

#[test]
fn graph_store_backends_match_for_update_delete_rollback_and_restart() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-store-conformance-{}.db",
        std::process::id()
    ));
    let snapshot_path = path.with_extension("snapshot");
    let sqlite_path = path.with_extension("sqlite");
    let mut memory = InMemoryGraphStore::new();
    let memory_snapshot = exercise_store(&mut memory)
        .map_err(|error| StoreError::Integrity(format!("InMemory fixture: {error}")))?;

    let mut file = FileGraphStore::open(&snapshot_path)?;
    let file_snapshot = exercise_store(&mut file)
        .map_err(|error| StoreError::Integrity(format!("File fixture: {error}")))?;
    drop(file);
    let reopened_file = FileGraphStore::open(&snapshot_path)?;
    let file_restart_snapshot = read_snapshot(&reopened_file, file_snapshot.manifest.generation)?;

    let mut turso = open_turso(&path)?;
    let turso_snapshot = exercise_store(&mut turso)
        .map_err(|error| StoreError::Integrity(format!("Turso fixture: {error}")))?;
    drop(turso);
    let reopened_turso = open_turso(&path)?;
    let turso_restart_snapshot =
        read_snapshot(&reopened_turso, turso_snapshot.manifest.generation)?;
    let mut sqlite = open_migrated(&sqlite_path)?;
    let sqlite_snapshot = exercise_store(&mut sqlite)
        .map_err(|error| StoreError::Integrity(format!("SQLite fixture: {error}")))?;
    drop(sqlite);
    let reopened_sqlite = open_migrated(&sqlite_path)?;
    let sqlite_restart_snapshot =
        read_snapshot(&reopened_sqlite, sqlite_snapshot.manifest.generation)?;
    for store in [
        &reopened_file as &dyn GraphStore,
        &reopened_turso,
        &reopened_sqlite,
    ] {
        let generation = first_delta().next_generation;
        let historical_file = file_version(b"first");
        let provenance = source_provenance();
        let query = Query::new(store, generation);
        verify_historical_search(store)?;
        let replacement_query = Query::new(store, replacement_delta(generation).next_generation);
        let mut revised_provenance = provenance.clone();
        revised_provenance.producer_version = "2".to_owned();
        revised_provenance.evidence_class = EvidenceClass::SemanticInference;
        if query
            .historical_file(historical_file.file_id)
            .map_err(|error| StoreError::Backend(error.to_string()))?
            != Some(historical_file)
            || query
                .historical_provenance(provenance.id)
                .map_err(|error| StoreError::Backend(error.to_string()))?
                != Some(provenance)
            || replacement_query
                .historical_provenance(revised_provenance.id)
                .map_err(|error| StoreError::Backend(error.to_string()))?
                != Some(revised_provenance)
        {
            return Err(StoreError::Integrity(
                "historical evidence was lost on restart".to_owned(),
            ));
        }
    }
    drop(reopened_file);
    drop(reopened_turso);
    drop(reopened_sqlite);
    std::fs::remove_file(&snapshot_path).map_err(|error| {
        StoreError::Backend(format!("remove file conformance snapshot: {error}"))
    })?;
    std::fs::remove_file(&path)
        .map_err(|error| StoreError::Backend(format!("remove Turso conformance DB: {error}")))?;
    std::fs::remove_file(&sqlite_path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite conformance DB: {error}")))?;

    if memory_snapshot != file_snapshot
        || memory_snapshot != file_restart_snapshot
        || memory_snapshot != turso_snapshot
        || memory_snapshot != turso_restart_snapshot
        || memory_snapshot != sqlite_snapshot
        || memory_snapshot != sqlite_restart_snapshot
    {
        return Err(StoreError::Integrity(format!(
            "graph-store differential mismatch: file={}, file_restart={}, turso={}, turso_restart={}, sqlite={}, sqlite_restart={}; restart_fields=(manifest {}, files {}, provenance {}, nodes {}, edges {})",
            memory_snapshot == file_snapshot,
            memory_snapshot == file_restart_snapshot,
            memory_snapshot == turso_snapshot,
            memory_snapshot == turso_restart_snapshot,
            memory_snapshot == sqlite_snapshot,
            memory_snapshot == sqlite_restart_snapshot,
            memory_snapshot.manifest == turso_restart_snapshot.manifest,
            memory_snapshot.files == turso_restart_snapshot.files,
            memory_snapshot.provenance == turso_restart_snapshot.provenance,
            memory_snapshot.nodes == turso_restart_snapshot.nodes,
            memory_snapshot.edges == turso_restart_snapshot.edges,
        )));
    }
    Ok(())
}

#[test]
fn reference_fact_variants_round_trip_across_all_store_backends() -> Result<(), StoreError> {
    let suffix = std::process::id().to_string();
    let database_path = std::env::temp_dir().join(format!("syntaxmesh-reference-{suffix}.db"));
    let file_path = database_path.with_extension("snapshot");
    let sqlite_path = database_path.with_extension("sqlite");

    let mut memory = InMemoryGraphStore::new();
    let memory_snapshot = publish_reference_facts(&mut memory)?;

    let mut file = FileGraphStore::open(&file_path)?;
    let file_snapshot = publish_reference_facts(&mut file)?;
    drop(file);
    let reopened_file = FileGraphStore::open(&file_path)?;
    let file_restart = read_snapshot(&reopened_file, file_snapshot.manifest.generation)?;

    let mut turso = open_turso(&database_path)?;
    let turso_snapshot = publish_reference_facts(&mut turso)?;
    drop(turso);
    let reopened_turso = open_turso(&database_path)?;
    let turso_restart = read_snapshot(&reopened_turso, turso_snapshot.manifest.generation)?;

    let mut sqlite = open_migrated(&sqlite_path)?;
    let sqlite_snapshot = publish_reference_facts(&mut sqlite)?;
    drop(sqlite);
    let reopened_sqlite = open_migrated(&sqlite_path)?;
    let sqlite_restart = read_snapshot(&reopened_sqlite, sqlite_snapshot.manifest.generation)?;

    let exact_names = vec!["library::target".to_owned()];
    let terminal_names = vec!["target".to_owned()];
    let candidate_ids =
        |store: &dyn GraphStore, generation: GenerationId| -> Result<Vec<NodeId>, StoreError> {
            let mut ids = store
                .symbol_candidates(generation, &exact_names, &terminal_names)?
                .into_iter()
                .map(|node| node.id)
                .collect::<Vec<_>>();
            ids.sort_unstable();
            Ok(ids)
        };
    let expected_candidates = vec![
        NodeId::derive(&[b"reference-resolved-occurrence"]),
        NodeId::derive(&[b"reference-target"]),
    ];
    let mut expected_candidates = expected_candidates;
    expected_candidates.sort_unstable();
    let candidate_lookups = [
        candidate_ids(&memory, memory_snapshot.manifest.generation)?,
        candidate_ids(&reopened_file, file_restart.manifest.generation)?,
        candidate_ids(&reopened_turso, turso_restart.manifest.generation)?,
        candidate_ids(&reopened_sqlite, sqlite_restart.manifest.generation)?,
    ];

    let matches = [
        &file_snapshot,
        &file_restart,
        &turso_snapshot,
        &turso_restart,
        &sqlite_snapshot,
        &sqlite_restart,
    ]
    .into_iter()
    .all(|snapshot| snapshot == &memory_snapshot);
    let runtime_fact = FactRef::Node(NodeId::derive(&[b"reference-runtime-observation"]));
    let temporal_axes_match = memory_snapshot
        .fact_histories
        .iter()
        .find(|(fact, _)| *fact == runtime_fact)
        .is_some_and(|(_, versions)| {
            matches!(versions.as_slice(), [version]
                if version.observed_at == Some(syntaxmesh_core::ObservationTime(123))
                    && version.accepted_at.is_none())
        });
    drop(reopened_file);
    drop(reopened_turso);
    drop(reopened_sqlite);
    std::fs::remove_file(&file_path)
        .map_err(|error| StoreError::Backend(format!("remove file reference fixture: {error}")))?;
    std::fs::remove_file(&database_path)
        .map_err(|error| StoreError::Backend(format!("remove Turso reference fixture: {error}")))?;
    std::fs::remove_file(&sqlite_path).map_err(|error| {
        StoreError::Backend(format!("remove SQLite reference fixture: {error}"))
    })?;
    if !matches
        || !temporal_axes_match
        || candidate_lookups
            .iter()
            .any(|ids| ids != &expected_candidates)
    {
        return Err(StoreError::Integrity(
            "reference-fact backend, restart state, or symbol lookup differs".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn durable_historical_snapshots_use_temporal_versions_across_checkpoint_depth()
-> Result<(), StoreError> {
    let suffix = std::process::id().to_string();
    let turso_path = std::env::temp_dir().join(format!("syntaxmesh-checkpoints-{suffix}.db"));
    let sqlite_path = turso_path.with_extension("sqlite");
    for path in [&turso_path, &sqlite_path] {
        let mut store: Box<dyn GraphStore> = if path == &turso_path {
            Box::new(open_turso(path)?)
        } else {
            Box::new(open_migrated(path)?)
        };
        let first = store.apply_delta(first_delta())?;
        let target = first.generation;
        let first_node = NodeId::derive(&[b"conformance-node-first"]);
        for sequence in 2_u64..=70 {
            let base = store
                .generation_history()?
                .last()
                .map(|entry| entry.manifest.generation)
                .ok_or_else(|| StoreError::Integrity("checkpoint history is empty".to_owned()))?;
            store.apply_delta(GraphDelta {
                repository: repository(),
                worktree: worktree(),
                run_id: IndexRunId::derive(&[&sequence.to_le_bytes()]),
                expected_base: Some(base),
                next_generation: GenerationId::derive(&[&sequence.to_le_bytes()]),
                changed_files: Vec::new(),
                removed_files: Vec::new(),
                upsert_provenance: Vec::new(),
                upsert_nodes: Vec::new(),
                upsert_edges: Vec::new(),
                remove_nodes: Vec::new(),
                remove_edges: Vec::new(),
            })?;
        }
        let historical = store.historical_snapshot(target)?;
        let node_history = store.node_history(first_node)?;
        let latest = store
            .generation_history()?
            .last()
            .map(|entry| entry.manifest.generation)
            .ok_or_else(|| StoreError::Integrity("checkpoint history is empty".to_owned()))?;
        let changes = store.changes_between(target, latest)?;
        let expected_match = vec![node(
            "first",
            file_version(b"first").file_id,
            source_provenance().id,
        )];
        if store.historical_search_nodes(target, "FIRST", 10)? != expected_match
            || store.historical_search_nodes(latest, "IrS", 10)? != expected_match
        {
            return Err(StoreError::Integrity(
                "historical search changed across checkpoint depth".to_owned(),
            ));
        }
        if historical.nodes.len() != 2 || historical.edges.len() != 1 {
            return Err(StoreError::Integrity(
                "checkpoint reconstruction returned an incorrect historical graph".to_owned(),
            ));
        }
        if node_history.len() != 1
            || node_history.first().is_none_or(|version| {
                version.valid_from != target
                    || version.valid_until.is_some()
                    || version.node.id != first_node
            })
            || changes.len() != 69
        {
            return Err(StoreError::Integrity(
                "temporal history or bounded range query returned incomplete results".to_owned(),
            ));
        }
        drop(store);
    }
    std::fs::remove_file(&turso_path).map_err(|error| {
        StoreError::Backend(format!("remove Turso checkpoint fixture: {error}"))
    })?;
    std::fs::remove_file(&sqlite_path).map_err(|error| {
        StoreError::Backend(format!("remove SQLite checkpoint fixture: {error}"))
    })?;
    Ok(())
}

#[test]
fn cold_database_copies_restore_current_and_historical_graphs() -> Result<(), StoreError> {
    let scratch = TestScratchDirectory::new("cold-copy-restore")?;
    for (backend_name, extension) in [("sqlite", "sqlite"), ("turso", "turso")] {
        let source = scratch
            .path
            .join(format!("source-{backend_name}.{extension}"));
        let backup = scratch
            .path
            .join(format!("backup-{backend_name}.{extension}"));
        let (first_generation, current_generation, expected_first, expected_current) =
            if backend_name == "sqlite" {
                let mut store = open_migrated(&source)?;
                let generations = write_backup_fixture(&mut store)?;
                drop(store);
                generations
            } else {
                let mut store = open_turso(&source)?;
                let generations = write_backup_fixture(&mut store)?;
                drop(store);
                generations
            };

        copy_cold_database_set(&source, &backup)?;
        if backend_name == "sqlite" {
            let restored = SqliteGraphStore::open(&backup)?;
            verify_restored_backup(
                &restored,
                first_generation,
                current_generation,
                &expected_first,
                &expected_current,
            )?;
        } else {
            let restored = TursoGraphStore::open(&backup)
                .map_err(|error| StoreError::Backend(format!("open cold Turso copy: {error}")))?;
            verify_restored_backup(
                &restored,
                first_generation,
                current_generation,
                &expected_first,
                &expected_current,
            )?;
        }
    }
    Ok(())
}

fn write_backup_fixture<S: DurableRecordStore + GraphStore>(
    store: &mut S,
) -> Result<(GenerationId, GenerationId, GraphSnapshot, GraphSnapshot), StoreError> {
    store.compare_exchange_record("test.backup/workflow", None, b"completed-marker")?;
    let first_generation = store.apply_delta(first_delta())?.generation;
    let current_generation = store
        .apply_delta(replacement_delta(first_generation))?
        .generation;
    Ok((
        first_generation,
        current_generation,
        store.historical_snapshot(first_generation)?,
        store.historical_snapshot(current_generation)?,
    ))
}

fn verify_restored_backup<S: BackendIntegrityCheck + DurableRecordStore + GraphStore>(
    store: &S,
    first_generation: GenerationId,
    current_generation: GenerationId,
    expected_first: &GraphSnapshot,
    expected_current: &GraphSnapshot,
) -> Result<(), StoreError> {
    let integrity = store.backend_integrity_check()?;
    if !integrity.passed
        || store.historical_snapshot(first_generation)? != *expected_first
        || store.historical_snapshot(current_generation)? != *expected_current
        || store
            .generation_history()?
            .last()
            .is_none_or(|entry| entry.manifest.generation != current_generation)
        || store.read_record("test.backup/workflow")?.as_deref() != Some(b"completed-marker")
    {
        return Err(StoreError::Integrity(format!(
            "restored {} cold backup failed physical integrity or current/historical graph checks: {:?}",
            integrity.kind.as_str(),
            integrity.findings
        )));
    }
    Ok(())
}

fn copy_cold_database_set(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), StoreError> {
    for suffix in ["", "-wal", "-shm"] {
        let source_file = path_with_suffix(source, suffix);
        if source_file.exists() {
            let destination_file = path_with_suffix(destination, suffix);
            std::fs::copy(&source_file, &destination_file).map_err(|error| {
                StoreError::Backend(format!(
                    "copy cold database file {} to {}: {error}",
                    source_file.display(),
                    destination_file.display()
                ))
            })?;
        } else if suffix.is_empty() {
            return Err(StoreError::Backend(format!(
                "cold database source is missing: {}",
                source_file.display()
            )));
        }
    }
    Ok(())
}

fn path_with_suffix(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    value.into()
}

struct TestScratchDirectory {
    path: std::path::PathBuf,
}

impl TestScratchDirectory {
    fn new(label: &str) -> Result<Self, StoreError> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("syntaxmesh-{label}-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).map_err(|error| {
            StoreError::Backend(format!("create test scratch directory: {error}"))
        })?;
        Ok(Self { path })
    }
}

impl Drop for TestScratchDirectory {
    fn drop(&mut self) {
        let _ignored = std::fs::remove_dir_all(&self.path);
    }
}

fn publish_reference_facts(store: &mut dyn GraphStore) -> Result<LogicalSnapshot, StoreError> {
    let repository = RepositoryId::derive(&[b"reference-repository"]);
    let worktree = WorktreeId::derive(&[b"reference-worktree"]);
    let generation = GenerationId::derive(&[b"reference-generation"]);
    let file_id = FileId::derive(&[b"src/caller.rs"]);
    let content_hash = syntaxmesh_core::StableId::derive("reference-content", &[b"caller();"]).0;
    let provenance_id = ProvenanceId::derive(&[b"reference-provenance"]);
    let runtime_provenance_id = ProvenanceId::derive(&[b"reference-runtime-provenance"]);
    let caller = NodeId::derive(&[b"reference-caller"]);
    let target = NodeId::derive(&[b"reference-target"]);
    let resolved = NodeId::derive(&[b"reference-resolved-occurrence"]);
    let unresolved = NodeId::derive(&[b"reference-unresolved-occurrence"]);
    let ambiguous = NodeId::derive(&[b"reference-ambiguous-occurrence"]);
    let runtime_observation = NodeId::derive(&[b"reference-runtime-observation"]);
    let module = NodeId::derive(&[b"reference-module"]);
    let import = NodeId::derive(&[b"reference-import-occurrence"]);
    let export = NodeId::derive(&[b"reference-export-occurrence"]);
    let import_diagnostic = NodeId::derive(&[b"reference-import-diagnostic"]);
    let script = NodeId::derive(&[b"reference-script"]);
    let document = NodeId::derive(&[b"reference-document"]);
    let section = NodeId::derive(&[b"reference-section"]);
    let document_link = NodeId::derive(&[b"reference-document-link"]);
    let linked_document = NodeId::derive(&[b"reference-linked-document"]);
    let diagnostic_provenance_id = ProvenanceId::derive(&[b"reference-diagnostic-provenance"]);
    let source = |id, kind, name: &str, span_start: u64| Node {
        id,
        kind,
        name: name.to_owned(),
        owner_file: Some(file_id),
        source: Some(syntaxmesh_core::SourceLocation {
            file_id,
            content_hash,
            span: syntaxmesh_core::SourceSpan {
                start_byte: span_start,
                end_byte: span_start.saturating_add(1),
            },
        }),
        provenance: provenance_id,
        extension_payload: None,
    };
    let caller_node = Node {
        id: caller,
        kind: NodeKind::Function,
        name: "caller".to_owned(),
        owner_file: Some(file_id),
        source: None,
        provenance: provenance_id,
        extension_payload: None,
    };
    let target_node = Node {
        id: target,
        kind: NodeKind::Function,
        name: "library::target".to_owned(),
        owner_file: Some(file_id),
        source: None,
        provenance: provenance_id,
        extension_payload: None,
    };
    let resolved_node = source(
        resolved,
        NodeKind::Reference {
            relation: RelationKind::Calls,
        },
        "target",
        0,
    );
    let unresolved_node = source(
        unresolved,
        NodeKind::UnresolvedReference {
            relation: RelationKind::Calls,
        },
        "missing",
        1,
    );
    let ambiguous_node = source(
        ambiguous,
        NodeKind::AmbiguousReference {
            relation: RelationKind::Calls,
        },
        "duplicate",
        2,
    );
    let module_node = Node {
        id: module,
        kind: NodeKind::Module,
        name: "src/caller.ts".to_owned(),
        owner_file: Some(file_id),
        source: None,
        provenance: provenance_id,
        extension_payload: None,
    };
    let import_node = source(
        import,
        NodeKind::Import {
            specifier: ".library".to_owned(),
            kind: ImportKind::PythonFrom,
            imported_name: Some("target".to_owned()),
            local_name: Some("localTarget".to_owned()),
            type_only: false,
        },
        "localTarget",
        3,
    );
    let export_node = source(
        export,
        NodeKind::Export {
            source_specifier: None,
            kind: ExportKind::Local,
            exported_name: Some("publicTarget".to_owned()),
            local_name: Some("localTarget".to_owned()),
            type_only: false,
        },
        "publicTarget",
        4,
    );
    let mut diagnostic_node = source(
        import_diagnostic,
        NodeKind::ModuleResolutionDiagnostic {
            occurrence: import,
            status: syntaxmesh_core::ModuleResolutionDiagnosticStatus::Unresolved,
            candidate_paths: Vec::new(),
        },
        "./library.ts",
        5,
    );
    diagnostic_node.provenance = diagnostic_provenance_id;
    let runtime_node = Node {
        id: runtime_observation,
        kind: NodeKind::RuntimeObservation,
        name: "caller invoked target".to_owned(),
        owner_file: None,
        source: None,
        provenance: runtime_provenance_id,
        extension_payload: Some(syntaxmesh_core::ExtensionPayload {
            namespace: "syntaxmesh.runtime.fixture".to_owned(),
            schema_version: 1,
            bytes:
                br#"{"schema_version":1,"observed_at_unix_nanos":123,"subject":"caller","relation":"invoked","object":"target"}"#
                    .to_vec(),
        }),
    };
    let mut second_runtime_node = runtime_node.clone();
    second_runtime_node.id = NodeId::derive(&[b"reference-runtime-observation-second"]);
    second_runtime_node.name = "second observed event".to_owned();
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"reference-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: vec![FileVersion {
            file_id,
            normalized_path: "src/caller.rs".to_owned(),
            content_hash,
            size_bytes: 9,
        }],
        removed_files: Vec::new(),
        upsert_provenance: vec![
            Provenance {
                id: provenance_id,
                producer_namespace: "syntaxmesh.reference-fixture".to_owned(),
                producer_version: "1".to_owned(),
                evidence_class: EvidenceClass::SourceFact,
                source: None,
            },
            Provenance {
                id: runtime_provenance_id,
                producer_namespace: "syntaxmesh.runtime.fixture".to_owned(),
                producer_version: "1".to_owned(),
                evidence_class: EvidenceClass::RuntimeObserved,
                source: None,
            },
            Provenance {
                id: diagnostic_provenance_id,
                producer_namespace: "syntaxmesh.reference-resolver".to_owned(),
                producer_version: "1".to_owned(),
                evidence_class: EvidenceClass::ResolutionDiagnostic,
                source: diagnostic_node.source.clone(),
            },
        ],
        upsert_nodes: vec![
            caller_node,
            target_node,
            module_node,
            resolved_node,
            unresolved_node,
            ambiguous_node,
            import_node,
            export_node,
            diagnostic_node,
            source(script, NodeKind::Script, "scripts/build.sh", 6),
            source(document, NodeKind::Document, "docs/guide.md", 7),
            source(section, NodeKind::Section, "Runtime", 8),
            source(
                document_link,
                NodeKind::Reference {
                    relation: RelationKind::References,
                },
                "docs/linked.md",
                9,
            ),
            source(linked_document, NodeKind::Document, "docs/linked.md", 10),
            runtime_node,
            second_runtime_node,
        ],
        upsert_edges: vec![
            edge(
                caller,
                resolved,
                RelationKind::References,
                provenance_id,
                b"resolved-source",
            ),
            edge(
                resolved,
                target,
                RelationKind::ResolvesTo,
                provenance_id,
                b"resolved-target",
            ),
            edge(
                caller,
                target,
                RelationKind::Calls,
                provenance_id,
                b"resolved-call",
            ),
            edge(
                caller,
                unresolved,
                RelationKind::References,
                provenance_id,
                b"unresolved",
            ),
            edge(
                caller,
                ambiguous,
                RelationKind::References,
                provenance_id,
                b"ambiguous",
            ),
            edge(
                module,
                import,
                RelationKind::Imports,
                provenance_id,
                b"source-import",
            ),
            edge(
                module,
                export,
                RelationKind::Exports,
                provenance_id,
                b"source-export",
            ),
            edge(
                import,
                import_diagnostic,
                RelationKind::HasResolutionDiagnostic,
                diagnostic_provenance_id,
                b"module-resolution-diagnostic",
            ),
            edge(
                script,
                caller,
                RelationKind::Contains,
                provenance_id,
                b"script-function",
            ),
            edge(
                document,
                section,
                RelationKind::Contains,
                provenance_id,
                b"document-section",
            ),
            edge(
                document,
                document_link,
                RelationKind::References,
                provenance_id,
                b"document-link-occurrence",
            ),
            edge(
                document_link,
                linked_document,
                RelationKind::ResolvesTo,
                provenance_id,
                b"document-link-resolution",
            ),
            edge(
                document,
                linked_document,
                RelationKind::References,
                provenance_id,
                b"document-link-semantic",
            ),
        ],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let resolution_nodes = store.module_resolution_nodes(generation)?;
    if resolution_nodes.len() != 4
        || resolution_nodes.iter().any(|node| {
            !matches!(
                &node.kind,
                NodeKind::Module
                    | NodeKind::Import { .. }
                    | NodeKind::Export { .. }
                    | NodeKind::ModuleResolutionDiagnostic { .. }
            )
        })
    {
        return Err(StoreError::Integrity(
            "module-resolution inventory included unrelated facts or missed typed occurrences"
                .to_owned(),
        ));
    }
    let first_page =
        store.observed_facts_between(ObservationTime(100), ObservationTime(200), None, 1)?;
    let next_page = store.observed_facts_between(
        ObservationTime(100),
        ObservationTime(200),
        first_page.next_cursor,
        1,
    )?;
    let first_item = first_page.items.first();
    let next_item = next_page.items.first();
    if first_page.items.len() != 1
        || first_page.next_cursor.is_none()
        || next_page.items.len() != 1
        || next_page.next_cursor.is_some()
        || first_item.is_none_or(|item| item.version.observed_at != Some(ObservationTime(123)))
        || first_item.is_none_or(|item| item.version.accepted_at.is_some())
        || matches!((first_item, next_item), (Some(first), Some(next)) if first.fact == next.fact)
    {
        return Err(StoreError::Integrity(
            "observation timeline cursor skipped, duplicated, or rewrote event-time facts"
                .to_owned(),
        ));
    }
    read_snapshot(store, generation)
}

fn edge(
    source: NodeId,
    target: NodeId,
    relation: RelationKind,
    provenance: ProvenanceId,
    tag: &[u8],
) -> Edge {
    Edge {
        id: EdgeId::derive(&[tag]),
        source,
        target,
        relation,
        provenance,
        extension_payload: None,
    }
}

#[test]
fn temporal_incident_edge_cascade_batches_in_sqlite_and_turso() -> Result<(), StoreError> {
    const REMOVED_NODE_COUNT: usize = 260;
    let suffix = std::process::id().to_string();
    let sqlite_path = std::env::temp_dir().join(format!(
        "syntaxmesh-sqlite-temporal-cascade-batch-{suffix}.db"
    ));
    let turso_path = std::env::temp_dir().join(format!(
        "syntaxmesh-turso-temporal-cascade-batch-{suffix}.db"
    ));
    let mut initial = first_delta();
    let first_generation = initial.next_generation;
    let file_id = FileId::derive(&[b"conformance-file"]);
    let provenance_id = ProvenanceId::derive(&[b"conformance-source"]);
    initial.upsert_edges.clear();
    let mut node_ids = initial
        .upsert_nodes
        .iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();
    for index in 0..REMOVED_NODE_COUNT {
        let item = node(&format!("temporal-cascade-{index}"), file_id, provenance_id);
        node_ids.push(item.id);
        initial.upsert_nodes.push(item);
    }
    for (index, pair) in node_ids.windows(2).enumerate() {
        let source = pair
            .first()
            .copied()
            .ok_or_else(|| StoreError::Integrity("cascade source is missing".to_owned()))?;
        let target = pair
            .get(1)
            .copied()
            .ok_or_else(|| StoreError::Integrity("cascade target is missing".to_owned()))?;
        initial.upsert_edges.push(edge(
            source,
            target,
            RelationKind::Calls,
            provenance_id,
            format!("temporal-cascade-edge-{index}").as_bytes(),
        ));
    }
    let hub_node = node_ids
        .first()
        .copied()
        .ok_or_else(|| StoreError::Integrity("cascade hub is missing".to_owned()))?;
    for index in 2..node_ids.len() {
        let target = node_ids
            .get(index)
            .copied()
            .ok_or_else(|| StoreError::Integrity("high-degree target is missing".to_owned()))?;
        initial.upsert_edges.push(edge(
            hub_node,
            target,
            RelationKind::Calls,
            provenance_id,
            format!("temporal-star-edge-{index}").as_bytes(),
        ));
    }
    let edge_ids = initial
        .upsert_edges
        .iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();

    let mut sqlite = open_migrated(&sqlite_path)?;
    sqlite.apply_delta(initial.clone())?;
    let mut turso = open_turso(&turso_path)?;
    turso.apply_delta(initial)?;
    for store in [&sqlite as &dyn GraphStore, &turso as &dyn GraphStore] {
        let page = store.historical_incident_edges(
            first_generation,
            hub_node,
            syntaxmesh_core::EdgeDirection::Outgoing,
            None,
            7,
        )?;
        let next = store.historical_incident_edges(
            first_generation,
            hub_node,
            syntaxmesh_core::EdgeDirection::Outgoing,
            page.items.last().map(|item| item.id),
            7,
        )?;
        if page.items.len() != 7
            || !page.has_more
            || next.items.is_empty()
            || page.items.last().is_some_and(|last| {
                next.items
                    .first()
                    .is_some_and(|following| last.id >= following.id)
            })
        {
            return Err(StoreError::Integrity(
                "historical incidence page was not ordered, bounded, or continued".to_owned(),
            ));
        }
    }
    let removed_generation = GenerationId::derive(&[b"temporal-cascade-removed-generation"]);
    let removal = GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"temporal-cascade-removal-run"]),
        expected_base: Some(first_generation),
        next_generation: removed_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: node_ids,
        remove_edges: Vec::new(),
    };
    sqlite.apply_delta(removal.clone())?;
    turso.apply_delta(removal)?;
    let sqlite_prior = sqlite.historical_snapshot(first_generation)?;
    let sqlite_current = sqlite.historical_snapshot(removed_generation)?;
    let turso_prior = turso.historical_snapshot(first_generation)?;
    let turso_current = turso.historical_snapshot(removed_generation)?;
    for edge_id in edge_ids.iter().copied() {
        for history in [
            sqlite.fact_history(FactRef::Edge(edge_id))?,
            turso.fact_history(FactRef::Edge(edge_id))?,
        ] {
            if history.len() != 1
                || history.first().is_none_or(|version| {
                    version.valid_from != first_generation
                        || version.valid_until != Some(removed_generation)
                })
            {
                return Err(StoreError::Integrity(format!(
                    "incident edge {edge_id:?} has an invalid temporal interval after node cascade"
                )));
            }
        }
    }
    drop(sqlite);
    drop(turso);
    for path in [&sqlite_path, &turso_path] {
        std::fs::remove_file(path).map_err(|error| {
            StoreError::Backend(format!("remove temporal cascade batch fixture: {error}"))
        })?;
    }
    if [&sqlite_prior, &turso_prior]
        .iter()
        .any(|prior| prior.edges.len() != edge_ids.len())
        || [&sqlite_current, &turso_current]
            .iter()
            .any(|current| !current.nodes.is_empty() || !current.edges.is_empty())
    {
        return Err(StoreError::Integrity(format!(
            "batched SQLite/Turso temporal cascade returned inconsistent historical/current graphs: SQLite prior/current edges={}/{}, Turso prior/current edges={}/{}",
            sqlite_prior.edges.len(),
            sqlite_current.edges.len(),
            turso_prior.edges.len(),
            turso_current.edges.len()
        )));
    }
    Ok(())
}

fn exercise_store(store: &mut dyn GraphStore) -> Result<LogicalSnapshot, StoreError> {
    let first = first_delta();
    let repository = first.repository;
    let worktree = first.worktree;
    let first_generation = first.next_generation;
    let first_node = NodeId::derive(&[b"conformance-node-first"]);
    let first_edge = EdgeId::derive(&[b"conformance-edge"]);
    let first_manifest = store.apply_delta_at(first, AcceptanceTime(10))?;
    let second_node = NodeId::derive(&[b"conformance-node-second"]);
    let file_id = FileId::derive(&[b"conformance-file"]);
    let file_nodes = store.nodes_for_file(first_generation, file_id)?;
    if store.outgoing(first_generation, first_node)?.len() != 1
        || store.incoming(first_generation, second_node)?.len() != 1
        || file_nodes.len() != 2
        || !file_nodes.contains(&first_node)
        || !file_nodes.contains(&second_node)
        || store.search_nodes(first_generation, "FIRST", 10)?
            != vec![node(
                "first",
                file_id,
                ProvenanceId::derive(&[b"conformance-source"]),
            )]
        || store.search_nodes(first_generation, "IrS", 10)?
            != vec![node(
                "first",
                file_id,
                ProvenanceId::derive(&[b"conformance-source"]),
            )]
    {
        return Err(StoreError::Integrity(
            "indexed graph reads differ from canonical facts".to_owned(),
        ));
    }
    if first_manifest.generation != first_generation {
        return Err(StoreError::Integrity(
            "first generation manifest differs from requested generation".to_owned(),
        ));
    }
    let first_history_state = store.historical_snapshot(first_generation)?;
    if first_history_state.nodes.len() != 2
        || first_history_state.edges.len() != 1
        || store
            .historical_node(first_generation, first_node)?
            .is_none()
    {
        return Err(StoreError::Integrity(
            "first historical graph state did not reconstruct".to_owned(),
        ));
    }

    let mut replacement = replacement_delta(first_generation);
    let mut revised_provenance = source_provenance();
    revised_provenance.producer_version = "2".to_owned();
    revised_provenance.evidence_class = EvidenceClass::SemanticInference;
    replacement.upsert_provenance = vec![revised_provenance.clone()];
    let second_generation = replacement.next_generation;
    store.apply_delta_at(replacement, AcceptanceTime(20))?;
    if store.historical_file(first_generation, file_id)? != Some(file_version(b"first"))
        || store.historical_file(second_generation, file_id)? != Some(file_version(b"replacement"))
        || store.historical_provenance(first_generation, source_provenance().id)?
            != Some(source_provenance())
        || store.historical_provenance(second_generation, source_provenance().id)?
            != Some(revised_provenance.clone())
        || store
            .historical_file(first_generation, FileId::derive(&[b"missing-file"]))?
            .is_some()
        || store
            .historical_provenance(
                first_generation,
                ProvenanceId::derive(&[b"missing-provenance"]),
            )?
            .is_some()
    {
        return Err(StoreError::Integrity(
            "historical evidence point reads differ across replacement".to_owned(),
        ));
    }
    let current_entry = store
        .current_generation_entry(repository, worktree)?
        .ok_or_else(|| StoreError::Integrity("current history entry is missing".to_owned()))?;
    if current_entry.manifest.generation != second_generation
        || current_entry
            .delta
            .as_ref()
            .and_then(|delta| delta.expected_base)
            != Some(first_generation)
    {
        return Err(StoreError::Integrity(
            "current history-entry lookup returned the wrong transition".to_owned(),
        ));
    }
    let first_node_history = store.node_history(first_node)?;
    let replacement_node = NodeId::derive(&[b"conformance-node-replacement"]);
    let changes = store.changes_between(first_generation, second_generation)?;
    let replaced_versions =
        store.fact_versions_changed_at(FactRef::Node(first_node), second_generation)?;
    let second_node_versions =
        store.fact_versions_changed_at(FactRef::Node(second_node), second_generation)?;
    let introduced_versions =
        store.fact_versions_changed_at(FactRef::Node(replacement_node), second_generation)?;
    let edge_versions_changed =
        store.fact_versions_changed_at(FactRef::Edge(first_edge), second_generation)?;
    let file_versions_changed = store.fact_versions_changed_at(
        FactRef::File(FileId::derive(&[b"conformance-file"])),
        second_generation,
    )?;
    let provenance_versions_changed = store.fact_versions_changed_at(
        FactRef::Provenance(ProvenanceId::derive(&[b"conformance-source"])),
        second_generation,
    )?;
    let mut changed_version_pages = Vec::new();
    let mut changed_version_cursor = None;
    loop {
        let page =
            store.fact_version_changes_at_page(second_generation, changed_version_cursor, 1)?;
        changed_version_pages.extend(page.items);
        changed_version_cursor = page.next_cursor;
        if changed_version_cursor.is_none() {
            break;
        }
    }
    let mut expected_changed_versions = replaced_versions
        .iter()
        .chain(introduced_versions.iter())
        .chain(second_node_versions.iter())
        .chain(edge_versions_changed.iter())
        .chain(file_versions_changed.iter())
        .chain(provenance_versions_changed.iter())
        .cloned()
        .collect::<Vec<_>>();
    expected_changed_versions.sort_by_key(|entry| (entry.fact, entry.valid_from_sequence));
    let second_history_state = store.historical_snapshot(second_generation)?;
    let replacement_history_mismatch = second_history_state.nodes.len() != 1
        || !second_history_state.edges.is_empty()
        || second_history_state
            .nodes
            .first()
            .is_none_or(|item| item.name != "replacement")
        || store
            .historical_node(second_generation, first_node)?
            .is_some()
        || store
            .historical_node(
                second_generation,
                NodeId::derive(&[b"conformance-node-replacement"]),
            )?
            .is_none()
        || first_node_history.len() != 1
        || first_node_history.first().is_none_or(|version| {
            version.valid_from != first_generation || version.valid_until != Some(second_generation)
        })
        || changes.len() != 1
        || changes.first().is_none_or(|change| {
            change.manifest.generation != second_generation
                || !change.delta.remove_nodes.contains(&first_node)
                || !change
                    .delta
                    .upsert_nodes
                    .iter()
                    .any(|node| node.id == replacement_node)
        });
    let replacement_history_mismatch = replacement_history_mismatch
        || replaced_versions.len() != 1
        || replaced_versions.first().is_none_or(|entry| {
            entry.fact != FactRef::Node(first_node)
                || entry.version.valid_from != first_generation
                || entry.version.valid_until != Some(second_generation)
                || entry.valid_until_sequence.is_none()
        })
        || introduced_versions.len() != 1
        || introduced_versions.first().is_none_or(|entry| {
            entry.fact != FactRef::Node(replacement_node)
                || entry.version.valid_from != second_generation
                || entry.version.valid_until.is_some()
                || entry.valid_from_sequence <= 1
        });
    let replacement_history_mismatch = replacement_history_mismatch
        || changed_version_pages != expected_changed_versions
        || changed_version_pages.len() != expected_changed_versions.len();
    if replacement_history_mismatch {
        return Err(StoreError::Integrity(format!(
            "replacement historical graph state or timeline queries did not reconstruct: actual={changed_version_pages:?}, expected={expected_changed_versions:?}"
        )));
    }

    let mut rejected = deletion_delta(None);
    rejected.run_id = IndexRunId::derive(&[b"conformance-stale-run"]);
    if !matches!(
        store.apply_delta(rejected),
        Err(StoreError::StaleBase { .. })
    ) {
        return Err(StoreError::Integrity(
            "stale graph delta was not rejected consistently".to_owned(),
        ));
    }
    if store
        .current_generation(repository, worktree)?
        .map(|item| item.generation)
        != Some(second_generation)
    {
        return Err(StoreError::Integrity(
            "rejected stale delta changed the current generation".to_owned(),
        ));
    }

    let malformed = GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"conformance-invalid-run"]),
        expected_base: Some(second_generation),
        next_generation: GenerationId::derive(&[b"conformance-invalid-generation"]),
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: vec![Edge {
            id: EdgeId::derive(&[b"conformance-dangling-edge"]),
            source: first_node,
            target: NodeId::derive(&[b"conformance-missing-node"]),
            relation: RelationKind::Calls,
            provenance: ProvenanceId::derive(&[b"conformance-source"]),
            extension_payload: None,
        }],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    if !matches!(store.apply_delta(malformed), Err(StoreError::Integrity(_))) {
        return Err(StoreError::Integrity(
            "invalid graph delta did not fail referential validation".to_owned(),
        ));
    }
    if store.manifest(second_generation)?.generation != second_generation {
        return Err(StoreError::Integrity(
            "invalid delta changed the accepted generation".to_owned(),
        ));
    }

    let deletion = deletion_delta(Some(second_generation));
    let final_generation = deletion.next_generation;
    store.apply_delta(deletion)?;
    let replacement_history = store.node_history(replacement_node)?;
    let full_change_range = store.changes_between(first_generation, final_generation)?;
    let removed_versions =
        store.fact_versions_changed_at(FactRef::Node(replacement_node), final_generation)?;
    let mut final_changed_versions = Vec::new();
    let mut final_changed_cursor = None;
    loop {
        let page = store.fact_version_changes_at_page(final_generation, final_changed_cursor, 1)?;
        final_changed_versions.extend(page.items);
        final_changed_cursor = page.next_cursor;
        if final_changed_cursor.is_none() {
            break;
        }
    }
    let mismatched_fact_cursor = FactVersionChangeCursor {
        generation: second_generation,
        after_fact: FactRef::Node(replacement_node),
        after_valid_from_sequence: 2,
    };
    let fact_cursor_generation_rejected = matches!(
        store.fact_version_changes_at_page(final_generation, Some(mismatched_fact_cursor), 1),
        Err(StoreError::InvalidDelta(_))
    );
    let oversized_fact_page_rejected = matches!(
        store.fact_version_changes_at_page(
            final_generation,
            None,
            MAX_FACT_VERSION_CHANGE_PAGE_SIZE + 1,
        ),
        Err(StoreError::InvalidPageLimit)
    );
    let mut change_pages = Vec::new();
    let mut change_cursor = None;
    loop {
        let page =
            store.changes_between_page(first_generation, final_generation, change_cursor, 1)?;
        change_pages.extend(page.items);
        change_cursor = page.next_cursor;
        if change_cursor.is_none() {
            break;
        }
    }
    let invalid_change_cursor = GenerationChangeCursor {
        from_generation: first_generation,
        to_generation: second_generation,
        after_sequence: 3,
        after_generation: second_generation,
    };
    let cursor_range_rejected = matches!(
        store.changes_between_page(
            first_generation,
            final_generation,
            Some(invalid_change_cursor),
            1,
        ),
        Err(StoreError::InvalidDelta(_))
    );
    let oversized_page_rejected = matches!(
        store.changes_between_page(
            first_generation,
            final_generation,
            None,
            MAX_GENERATION_CHANGE_PAGE_SIZE + 1,
        ),
        Err(StoreError::InvalidPageLimit)
    );
    let snapshot = read_snapshot(store, final_generation)?;
    verify_historical_search(store)?;
    if store.historical_file(final_generation, file_id)?.is_some()
        || store.historical_file(first_generation, file_id)? != Some(file_version(b"first"))
        || store.historical_file(second_generation, file_id)? != Some(file_version(b"replacement"))
        || store.historical_provenance(final_generation, source_provenance().id)?
            != Some(revised_provenance)
    {
        return Err(StoreError::Integrity(
            "historical evidence validity changed after deletion".to_owned(),
        ));
    }
    let unknown_generation = GenerationId::derive(&[b"unknown-evidence-generation"]);
    if !matches!(
        store.historical_file(unknown_generation, file_id),
        Err(StoreError::StaleBase { .. })
    ) || !matches!(
        store.historical_provenance(unknown_generation, source_provenance().id),
        Err(StoreError::StaleBase { .. })
    ) {
        return Err(StoreError::Integrity(
            "unknown evidence generation was accepted".to_owned(),
        ));
    }
    if snapshot.history.len() != 3
        || snapshot.history.first().is_none_or(|entry| {
            entry.manifest.generation != first_generation || entry.delta.is_none()
        })
        || snapshot.history.get(1).is_none_or(|entry| {
            entry.manifest.parent != Some(first_generation) || entry.delta.is_none()
        })
        || snapshot.history.get(2).is_none_or(|entry| {
            entry.manifest.parent != Some(second_generation) || entry.delta.is_none()
        })
    {
        return Err(StoreError::Integrity(
            "accepted edit/delete generation history is incomplete".to_owned(),
        ));
    }
    if !snapshot.files.is_empty()
        || !snapshot.nodes.is_empty()
        || !snapshot.edges.is_empty()
        || !store
            .historical_snapshot(final_generation)?
            .nodes
            .is_empty()
        || store
            .historical_node(final_generation, first_node)?
            .is_some()
        || replacement_history.len() != 1
        || replacement_history.first().is_none_or(|version| {
            version.valid_from != second_generation || version.valid_until != Some(final_generation)
        })
        || full_change_range.len() != 2
        || removed_versions.len() != 1
        || removed_versions.first().is_none_or(|entry| {
            entry.version.valid_from != second_generation
                || entry.version.valid_until != Some(final_generation)
                || entry.valid_until_sequence.is_none()
        })
        || final_changed_versions.len() != 2
        || !final_changed_versions.iter().any(|entry| {
            entry.fact == FactRef::Node(replacement_node)
                && entry.version.valid_until == Some(final_generation)
        })
        || !final_changed_versions.iter().any(|entry| {
            entry.fact == FactRef::File(FileId::derive(&[b"conformance-file"]))
                && entry.version.valid_until == Some(final_generation)
        })
        || change_pages != full_change_range
        || change_pages
            .iter()
            .map(|change| change.sequence)
            .collect::<Vec<_>>()
            != [2, 3]
        || !cursor_range_rejected
        || !oversized_page_rejected
        || !fact_cursor_generation_rejected
        || !oversized_fact_page_rejected
        || store
            .fact_history(FactRef::File(FileId::derive(&[b"conformance-file"])))?
            .len()
            != 2
        || store
            .fact_history(FactRef::Provenance(ProvenanceId::derive(&[
                b"conformance-source",
            ])))?
            .is_empty()
        || store
            .fact_history(FactRef::Node(first_node))?
            .first()
            .is_none_or(|version| {
                version.valid_from != first_generation
                    || version.valid_until != Some(second_generation)
            })
        || store
            .fact_history(FactRef::Edge(first_edge))?
            .first()
            .is_none_or(|version| {
                version.valid_from != first_generation
                    || version.valid_until != Some(second_generation)
            })
        || store.outgoing(final_generation, first_node)? != Vec::new()
        || store
            .edges(final_generation)?
            .iter()
            .any(|edge| edge.id == first_edge)
    {
        return Err(StoreError::Integrity(format!(
            "file deletion left file-owned graph facts behind: changed={final_changed_versions:?}"
        )));
    }
    Ok(snapshot)
}

fn read_snapshot(
    store: &dyn GraphStore,
    generation: GenerationId,
) -> Result<LogicalSnapshot, StoreError> {
    Ok(LogicalSnapshot {
        manifest: store.manifest(generation)?,
        files: store.files(generation)?,
        provenance: store.provenance(generation)?,
        nodes: store.nodes(generation)?,
        edges: store.edges(generation)?,
        history: store.generation_history()?,
        accepted: store
            .accepted_generations_between(AcceptanceTime(0), AcceptanceTime(100), None, 100)?
            .items,
        fact_histories: [
            FactRef::File(FileId::derive(&[b"conformance-file"])),
            FactRef::Provenance(ProvenanceId::derive(&[b"conformance-source"])),
            FactRef::Node(NodeId::derive(&[b"conformance-node-first"])),
            FactRef::Edge(EdgeId::derive(&[b"conformance-edge"])),
            FactRef::Node(NodeId::derive(&[b"reference-runtime-observation"])),
            FactRef::Node(NodeId::derive(&[b"reference-import-occurrence"])),
            FactRef::Node(NodeId::derive(&[b"reference-export-occurrence"])),
        ]
        .into_iter()
        .map(|fact| Ok((fact, store.fact_history(fact)?)))
        .collect::<Result<Vec<_>, StoreError>>()?,
        observed_facts: store
            .observed_facts_between(ObservationTime(0), ObservationTime(1000), None, 100)?
            .items,
    })
}

fn verify_historical_search(store: &dyn GraphStore) -> Result<(), StoreError> {
    let first_generation = first_delta().next_generation;
    let second_generation = replacement_delta(first_generation).next_generation;
    let final_generation = deletion_delta(Some(second_generation)).next_generation;
    for generation in [first_generation, second_generation, final_generation] {
        let mut snapshot_nodes = store.historical_snapshot(generation)?.nodes;
        snapshot_nodes.sort_by_key(|node| node.id);
        for text in ["", "FIRST", "IrS", "replacement", "missing"] {
            for limit in [0, 1, 10] {
                let expected = snapshot_nodes
                    .iter()
                    .filter(|node| node.name.to_lowercase().contains(&text.to_lowercase()))
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>();
                let actual = Query::new(store, generation)
                    .historical_search(text, limit)
                    .map_err(|error| StoreError::Backend(error.to_string()))?;
                if actual != expected {
                    return Err(StoreError::Integrity(format!(
                        "historical search differs at {generation:?}, text={text:?}, limit={limit}: actual={actual:?}, expected={expected:?}"
                    )));
                }
            }
        }
    }
    for limit in [0, 1] {
        if !matches!(
            store.historical_search_nodes(
                GenerationId::derive(&[b"missing-search-generation"]),
                "",
                limit
            ),
            Err(StoreError::StaleBase { .. })
        ) {
            return Err(StoreError::Integrity(
                "historical search accepted an unknown generation".to_owned(),
            ));
        }
    }
    Ok(())
}

fn first_delta() -> GraphDelta {
    let source = source_provenance();
    let file = file_version(b"first");
    let first = node("first", file.file_id, source.id);
    let second = node("second", file.file_id, source.id);
    GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"conformance-first-run"]),
        expected_base: None,
        next_generation: GenerationId::derive(&[b"conformance-first-generation"]),
        changed_files: vec![file],
        removed_files: Vec::new(),
        upsert_provenance: vec![source],
        upsert_nodes: vec![first.clone(), second.clone()],
        upsert_edges: vec![Edge {
            id: EdgeId::derive(&[b"conformance-edge"]),
            source: first.id,
            target: second.id,
            relation: RelationKind::Calls,
            provenance: first.provenance,
            extension_payload: None,
        }],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    }
}

fn replacement_delta(expected_base: GenerationId) -> GraphDelta {
    let source = source_provenance();
    let file = file_version(b"replacement");
    let replacement = node("replacement", file.file_id, source.id);
    let removed_first = NodeId::derive(&[b"conformance-node-first"]);
    let removed_second = NodeId::derive(&[b"conformance-node-second"]);
    GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"conformance-replace-run"]),
        expected_base: Some(expected_base),
        next_generation: GenerationId::derive(&[b"conformance-second-generation"]),
        changed_files: vec![file],
        removed_files: Vec::new(),
        upsert_provenance: vec![source],
        upsert_nodes: vec![replacement],
        upsert_edges: Vec::new(),
        remove_nodes: vec![removed_first, removed_second],
        remove_edges: vec![EdgeId::derive(&[b"conformance-edge"])],
    }
}

fn deletion_delta(expected_base: Option<GenerationId>) -> GraphDelta {
    GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"conformance-delete-run"]),
        expected_base,
        next_generation: GenerationId::derive(&[b"conformance-final-generation"]),
        changed_files: Vec::new(),
        removed_files: vec![FileId::derive(&[b"conformance-file"])],
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: vec![NodeId::derive(&[b"conformance-node-replacement"])],
        remove_edges: Vec::new(),
    }
}

fn repository() -> RepositoryId {
    RepositoryId::derive(&[b"store-conformance-repository"])
}

fn worktree() -> WorktreeId {
    WorktreeId::derive(&[b"store-conformance-worktree"])
}

fn file_version(content: &[u8]) -> FileVersion {
    FileVersion {
        file_id: FileId::derive(&[b"conformance-file"]),
        normalized_path: "src/lib.rs".to_owned(),
        content_hash: syntaxmesh_core::StableId::derive("conformance-content", &[content]).0,
        size_bytes: u64::try_from(content.len()).unwrap_or_default(),
    }
}

fn source_provenance() -> Provenance {
    Provenance {
        id: ProvenanceId::derive(&[b"conformance-source"]),
        producer_namespace: "syntaxmesh.conformance".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    }
}

#[test]
fn durable_persistent_roots_preserve_canonical_order_for_large_fact_sets() -> Result<(), StoreError>
{
    let suffix = std::process::id().to_string();
    let turso_path = std::env::temp_dir().join(format!("syntaxmesh-root-order-{suffix}.db"));
    let sqlite_path = turso_path.with_extension("sqlite");
    let mut delta = first_delta();
    let generation = delta.next_generation;
    let file_id = FileId::derive(&[b"conformance-file"]);
    let provenance_id = ProvenanceId::derive(&[b"conformance-source"]);
    let mut ids = vec![NodeId::derive(&[b"conformance-node-first"])];
    for index in 0..64 {
        let item = node(&format!("root-order-{index}"), file_id, provenance_id);
        ids.push(item.id);
        delta.upsert_nodes.push(item);
    }
    for (index, pair) in ids.windows(2).enumerate() {
        let source = pair
            .first()
            .copied()
            .ok_or_else(|| StoreError::Integrity("root-order source is missing".to_owned()))?;
        let target = pair
            .get(1)
            .copied()
            .ok_or_else(|| StoreError::Integrity("root-order target is missing".to_owned()))?;
        delta.upsert_edges.push(edge(
            source,
            target,
            RelationKind::Calls,
            provenance_id,
            format!("root-order-edge-{index}").as_bytes(),
        ));
    }

    let mut memory = InMemoryGraphStore::new();
    memory.apply_delta(delta.clone())?;
    let expected = memory.historical_snapshot(generation)?;
    let mut turso = open_turso(&turso_path)?;
    turso.apply_delta(delta.clone())?;
    let turso_snapshot = turso.historical_snapshot(generation)?;
    drop(turso);
    let mut sqlite = open_migrated(&sqlite_path)?;
    sqlite.apply_delta(delta)?;
    let sqlite_snapshot = sqlite.historical_snapshot(generation)?;
    drop(sqlite);
    std::fs::remove_file(&turso_path).map_err(|error| {
        StoreError::Backend(format!("remove Turso root-order fixture: {error}"))
    })?;
    std::fs::remove_file(&sqlite_path).map_err(|error| {
        StoreError::Backend(format!("remove SQLite root-order fixture: {error}"))
    })?;
    if turso_snapshot != expected || sqlite_snapshot != expected {
        return Err(StoreError::Integrity(
            "durable persistent roots differ from canonical in-memory fact order".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn chronological_consequence_trace_matches_across_memory_sqlite_and_turso() -> Result<(), StoreError>
{
    let suffix = std::process::id().to_string();
    let sqlite_path = std::env::temp_dir().join(format!("syntaxmesh-trace-parity-{suffix}.db"));
    let turso_path =
        std::env::temp_dir().join(format!("syntaxmesh-trace-parity-turso-{suffix}.db"));
    let generations = [
        GenerationId::derive(&[b"trace-parity-g1"]),
        GenerationId::derive(&[b"trace-parity-g2"]),
        GenerationId::derive(&[b"trace-parity-g3"]),
    ];
    let [g1, g2, g3] = generations;
    let repository = repository();
    let worktree = worktree();
    let provenance_id = ProvenanceId::derive(&[b"trace-parity-provenance"]);
    let provenance = Provenance {
        id: provenance_id,
        producer_namespace: "syntaxmesh.trace.parity".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let mut origin = node(
        "trace-origin",
        FileId::derive(&[b"trace-parity-file"]),
        provenance_id,
    );
    let mut middle = node(
        "trace-middle",
        FileId::derive(&[b"trace-parity-file"]),
        provenance_id,
    );
    let mut target = node(
        "trace-target",
        FileId::derive(&[b"trace-parity-file"]),
        provenance_id,
    );
    origin.owner_file = None;
    middle.owner_file = None;
    target.owner_file = None;
    let origin_id = origin.id;
    let endpoint = |id| {
        LineageEndpoint::FactVersion(FactVersionRef {
            fact: FactRef::Node(id),
            valid_from: g1,
        })
    };
    let consequence = |name: &[u8], source, target_id| ConsequenceEdge {
        id: ConsequenceEdgeId::derive(&[name]),
        source: endpoint(source),
        target: endpoint(target_id),
        kind: ConsequenceKind::DirectDependencyEffect,
        evidence: vec![FactVersionRef {
            fact: FactRef::Node(source),
            valid_from: g1,
        }],
        derivation: ConsequenceDerivation::Explicit,
        provenance: provenance_id,
    };
    let first_edge = consequence(b"trace-parity-first", origin.id, middle.id);
    let delayed_edge = consequence(b"trace-parity-delayed", middle.id, target.id);
    let publish =
        |expected_base, next_generation, run_name: &[u8], upsert_provenance, upsert_nodes| {
            GraphDeltaWithLineage {
                graph: GraphDelta {
                    repository,
                    worktree,
                    run_id: IndexRunId::derive(&[run_name]),
                    expected_base,
                    next_generation,
                    changed_files: Vec::new(),
                    removed_files: Vec::new(),
                    upsert_provenance,
                    upsert_nodes,
                    upsert_edges: Vec::new(),
                    remove_nodes: Vec::new(),
                    remove_edges: Vec::new(),
                },
                lineage: Default::default(),
            }
        };
    let initial = GraphDeltaWithConsequences {
        publication: publish(
            None,
            g1,
            b"trace-parity-run-1",
            vec![provenance],
            vec![origin, middle, target],
        ),
        consequences: ConsequenceDelta {
            add: vec![first_edge.clone()],
            retract: Vec::new(),
        },
    };
    let retract = GraphDeltaWithConsequences {
        publication: publish(Some(g1), g2, b"trace-parity-run-2", Vec::new(), Vec::new()),
        consequences: ConsequenceDelta {
            add: Vec::new(),
            retract: vec![ConsequenceRetraction {
                edge: first_edge.id,
                provenance: provenance_id,
            }],
        },
    };
    let delayed = GraphDeltaWithConsequences {
        publication: publish(Some(g2), g3, b"trace-parity-run-3", Vec::new(), Vec::new()),
        consequences: ConsequenceDelta {
            add: vec![delayed_edge.clone()],
            retract: Vec::new(),
        },
    };

    let mut memory = InMemoryGraphStore::new();
    memory.apply_delta_with_consequences(initial.clone(), None)?;
    memory.apply_delta_with_consequences(retract.clone(), None)?;
    memory.apply_delta_with_consequences(delayed.clone(), None)?;
    SqliteGraphStore::migrate(&sqlite_path).map_err(|error| {
        StoreError::Backend(format!("migrate trace parity SQLite fixture: {error}"))
    })?;
    let mut sqlite = SqliteGraphStore::open(&sqlite_path).map_err(|error| {
        StoreError::Backend(format!("open trace parity SQLite fixture: {error}"))
    })?;
    sqlite.apply_delta_with_consequences(initial.clone(), None)?;
    sqlite.apply_delta_with_consequences(retract.clone(), None)?;
    sqlite.apply_delta_with_consequences(delayed.clone(), None)?;
    let mut turso = open_turso(&turso_path).map_err(|error| {
        StoreError::Backend(format!("open trace parity Turso fixture: {error}"))
    })?;
    turso.apply_delta_with_consequences(initial, None)?;
    turso.apply_delta_with_consequences(retract, None)?;
    turso.apply_delta_with_consequences(delayed, None)?;

    let request = ConsequenceTraceRequest {
        origin: endpoint(origin_id),
        from_generation: g1,
        until_generation: g3,
        max_hops: 8,
        max_endpoints: 20,
        max_edges: 20,
        max_scanned_incidences: 100,
        included_kinds: Vec::new(),
        included_evidence_classes: vec![EvidenceClass::UserAsserted],
    };
    let expected: ConsequenceTrace = read_consequence_trace(&memory, g3, &request)?;
    let sqlite_trace = read_consequence_trace(&sqlite, g3, &request)?;
    let turso_trace = read_consequence_trace(&turso, g3, &request)?;
    let absent_endpoint = endpoint(NodeId::derive(&[b"trace-parity-absent-endpoint"]));
    let sqlite_empty =
        sqlite.consequence_edges_for_endpoint_range(absent_endpoint, g1, g3, None, 10)?;
    let turso_empty =
        turso.consequence_edges_for_endpoint_range(absent_endpoint, g1, g3, None, 10)?;
    if !sqlite_empty.items.is_empty() || !turso_empty.items.is_empty() {
        return Err(StoreError::Integrity(
            "empty consequence range differs across durable backends".to_owned(),
        ));
    }
    let missing_generation = GenerationId::derive(&[b"trace-parity-missing-generation"]);
    if !matches!(
        sqlite.consequence_edges_for_endpoint_range(
            absent_endpoint,
            g1,
            missing_generation,
            None,
            10,
        ),
        Err(StoreError::StaleBase { .. })
    ) || !matches!(
        turso.consequence_edges_for_endpoint_range(
            absent_endpoint,
            g1,
            missing_generation,
            None,
            10,
        ),
        Err(StoreError::StaleBase { .. })
    ) {
        return Err(StoreError::Integrity(
            "missing consequence-range generation did not fail stale on both durable backends"
                .to_owned(),
        ));
    }
    if !matches!(
        sqlite.consequence_edges_for_endpoint_range(absent_endpoint, g3, g1, None, 10),
        Err(StoreError::InvalidDelta(_))
    ) || !matches!(
        turso.consequence_edges_for_endpoint_range(absent_endpoint, g3, g1, None, 10),
        Err(StoreError::InvalidDelta(_))
    ) {
        return Err(StoreError::Integrity(
            "reversed consequence range was not rejected consistently".to_owned(),
        ));
    }
    drop(sqlite);
    drop(turso);
    std::fs::remove_file(&sqlite_path)
        .map_err(|error| StoreError::Backend(format!("remove SQLite trace fixture: {error}")))?;
    std::fs::remove_file(&turso_path)
        .map_err(|error| StoreError::Backend(format!("remove Turso trace fixture: {error}")))?;

    let sqlite_semantics_match = sqlite_trace.origin == expected.origin
        && sqlite_trace.from_generation == expected.from_generation
        && sqlite_trace.until_generation == expected.until_generation
        && sqlite_trace.states == expected.states
        && sqlite_trace.hops == expected.hops
        && sqlite_trace.scanned_incidences == expected.scanned_incidences
        && sqlite_trace.truncated == expected.truncated;
    let turso_semantics_match = turso_trace.origin == expected.origin
        && turso_trace.from_generation == expected.from_generation
        && turso_trace.until_generation == expected.until_generation
        && turso_trace.states == expected.states
        && turso_trace.hops == expected.hops
        && turso_trace.scanned_incidences == expected.scanned_incidences
        && turso_trace.truncated == expected.truncated;
    if !sqlite_semantics_match
        || !turso_semantics_match
        || expected.truncated
        || !expected
            .hops
            .iter()
            .any(|hop| hop.version.edge.id == first_edge.id && hop.version.valid_until == Some(g2))
        || !expected
            .hops
            .iter()
            .any(|hop| hop.version.edge.id == delayed_edge.id && hop.reached_generation == g3)
    {
        return Err(StoreError::Integrity(format!(
            "chronological consequence trace diverged across graph-store backends: sqlite_equal={}, turso_equal={}, expected_hops={}, sqlite_hops={}, turso_hops={}, expected_truncated={}, sqlite_truncated={}, turso_truncated={}",
            sqlite_semantics_match,
            turso_semantics_match,
            expected.hops.len(),
            sqlite_trace.hops.len(),
            turso_trace.hops.len(),
            expected.truncated,
            sqlite_trace.truncated,
            turso_trace.truncated,
        )));
    }
    Ok(())
}

fn read_consequence_trace<S: GraphStore>(
    store: &S,
    generation: GenerationId,
    request: &ConsequenceTraceRequest,
) -> Result<ConsequenceTrace, StoreError> {
    Query::new(store, generation)
        .consequence_trace(request)
        .map_err(|error| StoreError::Integrity(format!("trace parity query failed: {error}")))
}

fn node(name: &str, owner_file: FileId, provenance: ProvenanceId) -> Node {
    Node {
        id: NodeId::derive(&[format!("conformance-node-{name}").as_bytes()]),
        kind: NodeKind::Function,
        name: name.to_owned(),
        owner_file: Some(owner_file),
        source: None,
        provenance,
        extension_payload: None,
    }
}
