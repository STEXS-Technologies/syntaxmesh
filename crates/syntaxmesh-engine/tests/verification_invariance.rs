use std::error::Error;

use syntaxmesh_core::{
    FileId, FileVersion, GenerationId, GenerationStatus, IndexRunId, RepositoryId, WorktreeId,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::SourceFile;
use syntaxmesh_store::{DurableRecordStore, FileGraphStore, GraphStore};
use syntaxmesh_store_sqlite::SqliteGraphStore;
use syntaxmesh_store_turso::TursoGraphStore;
use syntaxmesh_workflow::WorkflowStatus;

#[derive(Clone, Copy)]
struct Scope {
    repository: RepositoryId,
    worktree: WorktreeId,
    generation: GenerationId,
    run_id: IndexRunId,
    next_generation: GenerationId,
    next_run_id: IndexRunId,
}

#[test]
fn statechronicle_toggle_preserves_durable_graph_and_query_results() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let file_id = FileId::derive(&[b"src/verification_invariant.rs"]);
    let first = source_file(file_id, "pub fn verification_invariant() -> usize { 42 }\n");
    let second = source_file(file_id, "pub fn verification_invariant() -> usize { 43 }\n");
    let repository = RepositoryId::derive(&[b"durable-verification-invariant-repository"]);
    let worktree = WorktreeId::derive(&[b"durable-verification-invariant-worktree"]);
    let generation = GenerationId::derive(&[b"durable-verification-invariant-generation"]);
    let run_id = IndexRunId::derive(&[b"durable-verification-invariant-run"]);
    let next_generation =
        GenerationId::derive(&[b"durable-verification-invariant-next-generation"]);
    let next_run_id = IndexRunId::derive(&[b"durable-verification-invariant-next-run"]);
    let scope = Scope {
        repository,
        worktree,
        generation,
        run_id,
        next_generation,
        next_run_id,
    };

    let turso_durable_path = temporary.path().join("turso-durable.db");
    let turso_verified_path = temporary.path().join("turso-verified.db");
    TursoGraphStore::migrate(&turso_durable_path)?;
    TursoGraphStore::migrate(&turso_verified_path)?;
    check_backend_pair(
        (
            TursoGraphStore::open(&turso_durable_path)?,
            TursoGraphStore::open(&turso_verified_path)?,
        ),
        || {
            Ok((
                TursoGraphStore::open(&turso_durable_path)?,
                TursoGraphStore::open(&turso_verified_path)?,
            ))
        },
        [&first, &second],
        scope,
        "Turso",
    )?;

    let sqlite_durable_path = temporary.path().join("sqlite-durable.db");
    let sqlite_verified_path = temporary.path().join("sqlite-verified.db");
    SqliteGraphStore::migrate(&sqlite_durable_path)?;
    SqliteGraphStore::migrate(&sqlite_verified_path)?;
    check_backend_pair(
        (
            SqliteGraphStore::open(&sqlite_durable_path)?,
            SqliteGraphStore::open(&sqlite_verified_path)?,
        ),
        || {
            Ok((
                SqliteGraphStore::open(&sqlite_durable_path)?,
                SqliteGraphStore::open(&sqlite_verified_path)?,
            ))
        },
        [&first, &second],
        scope,
        "SQLite",
    )?;

    let file_durable_path = temporary.path().join("file-durable.snapshot");
    let file_verified_path = temporary.path().join("file-verified.snapshot");
    check_backend_pair(
        (
            FileGraphStore::open(&file_durable_path)?,
            FileGraphStore::open(&file_verified_path)?,
        ),
        || {
            Ok((
                FileGraphStore::open(&file_durable_path)?,
                FileGraphStore::open(&file_verified_path)?,
            ))
        },
        [&first, &second],
        scope,
        "File",
    )
}

fn check_backend_pair<S, O>(
    (durable_backend, verified_backend): (S, S),
    mut reopen: O,
    files: [&SourceFile; 2],
    scope: Scope,
    backend: &str,
) -> Result<(), Box<dyn Error>>
where
    S: GraphStore + DurableRecordStore,
    O: FnMut() -> Result<(S, S), Box<dyn Error>>,
{
    let mut durable = SyntaxMeshEngine::new(
        durable_backend,
        RustExtractor,
        scope.repository,
        scope.worktree,
    );
    let durable_receipt = durable
        .index(
            std::slice::from_ref(files[0]),
            scope.run_id,
            scope.generation,
        )
        .map_err(|error| format!("{backend} durable first publication: {error}"))?;
    let durable_next_receipt = durable
        .index(
            std::slice::from_ref(files[1]),
            scope.next_run_id,
            scope.next_generation,
        )
        .map_err(|error| format!("{backend} durable successor publication: {error}"))?;
    let mut verified = SyntaxMeshEngine::new(
        verified_backend,
        RustExtractor,
        scope.repository,
        scope.worktree,
    )
    .with_statechronicle_verification();
    let verified_receipt = verified
        .index(
            std::slice::from_ref(files[0]),
            scope.run_id,
            scope.generation,
        )
        .map_err(|error| format!("{backend} verified first publication: {error}"))?;
    let verified_next_receipt = verified
        .index(
            std::slice::from_ref(files[1]),
            scope.next_run_id,
            scope.next_generation,
        )
        .map_err(|error| format!("{backend} verified successor publication: {error}"))?;

    let durable_observations = engine_observations(&durable, scope)
        .map_err(|error| format!("{backend} durable historical observations: {error}"))?;
    let verified_observations = engine_observations(&verified, scope)
        .map_err(|error| format!("{backend} verified historical observations: {error}"))?;
    let durable_store = durable.into_store();
    let verified_store = verified.into_store();
    let same_graph = canonical_graph_matches(&durable_store, &verified_store, scope.generation)
        .map_err(|error| format!("{backend} first-generation graph comparison: {error}"))?;
    let same_latest_graph =
        canonical_graph_matches(&durable_store, &verified_store, scope.next_generation)
            .map_err(|error| format!("{backend} latest-generation graph comparison: {error}"))?;
    let same_history =
        durable_store.generation_history()? == verified_store.generation_history()?;
    let statuses_are_expected = [
        durable_receipt.verification,
        durable_next_receipt.verification,
    ] == [WorkflowStatus::Durable, WorkflowStatus::Durable]
        && [
            verified_receipt.verification,
            verified_next_receipt.verification,
        ] == [WorkflowStatus::Verified, WorkflowStatus::Verified]
        && durable_store
            .current_generation(scope.repository, scope.worktree)?
            .is_some_and(|manifest| manifest.status == GenerationStatus::Durable)
        && verified_store
            .current_generation(scope.repository, scope.worktree)?
            .is_some_and(|manifest| manifest.status == GenerationStatus::Verified);

    if !same_graph
        || !same_latest_graph
        || !same_history
        || durable_observations != verified_observations
        || !statuses_are_expected
    {
        return Err(format!(
            "{backend} verification toggle changed canonical history: first_graph={same_graph}, latest_graph={same_latest_graph}, generation_history={same_history}, historical_queries_equal={}, statuses={statuses_are_expected}",
            durable_observations == verified_observations,
        )
        .into());
    }

    drop(durable_store);
    drop(verified_store);
    let (reopened_durable_backend, reopened_verified_backend) = reopen()?;
    let reopened_durable = SyntaxMeshEngine::new(
        reopened_durable_backend,
        RustExtractor,
        scope.repository,
        scope.worktree,
    );
    let reopened_verified = SyntaxMeshEngine::new(
        reopened_verified_backend,
        RustExtractor,
        scope.repository,
        scope.worktree,
    );
    let reopened_durable_observations = engine_observations(&reopened_durable, scope)?;
    let reopened_verified_observations = engine_observations(&reopened_verified, scope)?;
    let reopened_durable_store = reopened_durable.into_store();
    let reopened_verified_store = reopened_verified.into_store();
    let same_reopened_graph = canonical_graph_matches(
        &reopened_durable_store,
        &reopened_verified_store,
        scope.generation,
    )?;
    let same_reopened_latest_graph = canonical_graph_matches(
        &reopened_durable_store,
        &reopened_verified_store,
        scope.next_generation,
    )?;
    let same_reopened_history = reopened_durable_store.generation_history()?
        == reopened_verified_store.generation_history()?;
    let reopened_statuses_are_expected = reopened_durable_store
        .current_generation(scope.repository, scope.worktree)?
        .is_some_and(|manifest| manifest.status == GenerationStatus::Durable)
        && reopened_verified_store
            .current_generation(scope.repository, scope.worktree)?
            .is_some_and(|manifest| manifest.status == GenerationStatus::Verified);
    if !same_reopened_graph
        || !same_reopened_latest_graph
        || !same_reopened_history
        || reopened_durable_observations != reopened_verified_observations
        || !reopened_statuses_are_expected
    {
        return Err(format!(
            "{backend} verification toggle failed restart equivalence: first_graph={same_reopened_graph}, latest_graph={same_reopened_latest_graph}, generation_history={same_reopened_history}, historical_queries_equal={}, statuses={reopened_statuses_are_expected}",
            reopened_durable_observations == reopened_verified_observations,
        )
        .into());
    }
    Ok(())
}

fn source_file(file_id: FileId, content: &str) -> SourceFile {
    SourceFile {
        file: FileVersion {
            file_id,
            normalized_path: "src/verification_invariant.rs".to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: content.len() as u64,
        },
        content: content.to_owned(),
    }
}

fn engine_observations<S>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    scope: Scope,
) -> Result<serde_json::Value, Box<dyn Error>>
where
    S: GraphStore + DurableRecordStore,
{
    let first = engine.query(scope.generation);
    let second = engine.query(scope.next_generation);
    let first_search = first
        .historical_search("verification_invariant", 16)
        .map_err(|error| format!("first-generation search: {error}"))?;
    let latest_search = second
        .search("verification_invariant", 16)
        .map_err(|error| format!("latest-generation search: {error}"))?;
    let first_graph = first
        .graph_at(scope.generation)
        .map_err(|error| format!("first graph-at read: {error}"))?;
    let latest_graph = second
        .graph_at(scope.next_generation)
        .map_err(|error| format!("latest graph-at read: {error}"))?;
    let changes = second
        .changed_between(scope.generation, scope.next_generation)
        .map_err(|error| format!("changed-between read: {error}"))?;
    Ok(serde_json::json!({
        "first_search": first_search,
        "latest_search": latest_search,
        "first_graph_at": first_graph,
        "latest_graph_at": latest_graph,
        "changes": changes,
    }))
}

fn canonical_graph_matches<S: GraphStore + ?Sized>(
    durable: &S,
    verified: &S,
    generation: GenerationId,
) -> Result<bool, Box<dyn Error>> {
    let durable_manifest = durable.manifest(generation)?;
    let mut verified_manifest = verified.manifest(generation)?;
    verified_manifest.status = durable_manifest.status;
    Ok(durable_manifest == verified_manifest
        && durable.historical_snapshot(generation)? == verified.historical_snapshot(generation)?)
}
