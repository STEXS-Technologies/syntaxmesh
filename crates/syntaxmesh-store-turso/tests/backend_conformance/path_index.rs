//! Reuse existing canonical context facts and durable backend factories.

use super::{
    FileGraphStore, GenerationId, GraphDelta, GraphStore, InMemoryGraphStore, IndexRunId,
    StoreError, open_migrated, open_turso, publish_context_fixture, repository, worktree,
};
use syntaxmesh_query::{GenerationIdentifierIndex, RankedCandidate};

#[path = "path_index/lexical.rs"]
mod lexical;

fn index(
    store: &dyn GraphStore,
    generation: GenerationId,
) -> Result<GenerationIdentifierIndex, StoreError> {
    GenerationIdentifierIndex::build_with_paths(store, generation, 2, 100, 10_000, 1, 1000)
        .map_err(|error| StoreError::Backend(error.to_string()))
}

fn ranked(
    store: &dyn GraphStore,
    generation: GenerationId,
    query: &str,
) -> Result<Vec<RankedCandidate>, StoreError> {
    index(store, generation)?
        .candidates(store, generation, query, 100, 2)
        .map_err(|error| StoreError::Backend(error.to_string()))
}

fn revise(store: &mut dyn GraphStore, base: GenerationId) -> Result<GenerationId, StoreError> {
    let mut snapshot = store.historical_snapshot(base)?;
    let revised = GenerationId::derive(&[b"path-index-revised"]);
    for file in &mut snapshot.files {
        file.normalized_path = "tools/revised.rs".to_owned();
        file.content_hash = [4; 32];
    }
    for node in &mut snapshot.nodes {
        node.name.push_str("_revised");
        if node.id == syntaxmesh_core::NodeId::derive(&[b"context-conformance-helper"]) {
            node.kind = syntaxmesh_core::NodeKind::DocumentChunk;
        }
        if let Some(source) = &mut node.source {
            source.content_hash = [4; 32];
        }
    }
    for provenance in &mut snapshot.provenance {
        provenance.producer_version = "2".to_owned();
        if let Some(source) = &mut provenance.source {
            source.content_hash = [4; 32];
        }
    }
    store.apply_delta(GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"path-index-revision"]),
        expected_base: Some(base),
        next_generation: revised,
        changed_files: snapshot.files,
        removed_files: Vec::new(),
        upsert_provenance: snapshot.provenance,
        upsert_nodes: snapshot.nodes,
        upsert_edges: snapshot.edges,
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    Ok(revised)
}

fn verify(
    store: &dyn GraphStore,
    first: GenerationId,
    second: GenerationId,
    expected: &[Vec<RankedCandidate>; 2],
) -> Result<(), StoreError> {
    if ranked(store, first, "src context")? != expected[0]
        || ranked(store, second, "tools revised")? != expected[1]
        || !ranked(store, second, "src context")?.is_empty()
        || !ranked(store, first, "tools revised")?.is_empty()
    {
        return Err(StoreError::Integrity(
            "path index mixed retained generations or backend payloads".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn path_rankings_conform_after_rename_edit_and_durable_restart() -> Result<(), StoreError> {
    let mut memory = InMemoryGraphStore::new();
    let first = publish_context_fixture(&mut memory)?;
    let cached = index(&memory, first)?;
    let cached_lexical = lexical::index(&memory, first)?;
    let before_lexical = lexical::rows(&cached_lexical, &memory, first)?;
    let before = ranked(&memory, first, "src context")?;
    let second = revise(&mut memory, first)?;
    let expected = [before.clone(), ranked(&memory, second, "tools revised")?];
    let after_lexical = lexical::rows(&lexical::index(&memory, second)?, &memory, second)?;
    if before_lexical == after_lexical
        || before_lexical.is_empty()
        || lexical::rows(&cached_lexical, &memory, first)? != before_lexical
    {
        return Err(StoreError::Integrity(
            "lexical fixture lost edit distinction or cached history".to_owned(),
        ));
    }
    lexical::verify(&memory, first, &before_lexical)?;
    lexical::verify(&memory, second, &after_lexical)?;
    if expected.iter().any(|rows| rows.len() != 2)
        || cached
            .candidates(&memory, first, "src context", 100, 2)
            .map_err(|error| StoreError::Backend(error.to_string()))?
            != before
    {
        return Err(StoreError::Integrity(
            "path fixture lost canonical candidates".to_owned(),
        ));
    }
    verify(&memory, first, second, &expected)?;
    let temporary = tempfile::tempdir().map_err(|error| StoreError::Backend(error.to_string()))?;
    for backend in ["file", "sqlite", "turso"] {
        let path = temporary.path().join(backend);
        let mut store: Box<dyn GraphStore> = match backend {
            "file" => Box::new(FileGraphStore::open(&path)?),
            "sqlite" => Box::new(open_migrated(&path)?),
            _ => Box::new(open_turso(&path)?),
        };
        publish_context_fixture(store.as_mut())?;
        let retained = index(store.as_ref(), first)?;
        let retained_lexical = lexical::index(store.as_ref(), first)?;
        revise(store.as_mut(), first)?;
        verify(store.as_ref(), first, second, &expected)?;
        lexical::verify(store.as_ref(), first, &before_lexical)?;
        lexical::verify(store.as_ref(), second, &after_lexical)?;
        if lexical::rows(&retained_lexical, store.as_ref(), first)? != before_lexical {
            return Err(StoreError::Integrity(
                "cached lexical planner changed historical generation".to_owned(),
            ));
        }
        if retained
            .candidates(store.as_ref(), first, "src context", 100, 2)
            .map_err(|error| StoreError::Backend(error.to_string()))?
            != before
        {
            return Err(StoreError::Integrity(
                "cached historical path index changed".to_owned(),
            ));
        }
        drop(store);
        let reopened: Box<dyn GraphStore> = match backend {
            "file" => Box::new(FileGraphStore::open(&path)?),
            "sqlite" => Box::new(super::SqliteGraphStore::open(&path)?),
            _ => Box::new(
                super::TursoGraphStore::open(&path)
                    .map_err(|error| StoreError::Backend(error.to_string()))?,
            ),
        };
        verify(reopened.as_ref(), first, second, &expected)?;
        lexical::verify(reopened.as_ref(), first, &before_lexical)?;
        lexical::verify(reopened.as_ref(), second, &after_lexical)?;
    }
    Ok(())
}
