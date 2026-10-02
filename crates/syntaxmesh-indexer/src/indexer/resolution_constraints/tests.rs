use super::*;
use syntaxmesh_core::ExtensionPayload;
use syntaxmesh_store::{FileGraphStore, InMemoryGraphStore};

#[test]
fn malformed_persisted_constraint_rejects_reresolution_without_publication()
-> Result<(), IndexError> {
    for (schema_version, bytes) in [(2, vec![1]), (1, vec![]), (1, vec![0])] {
        require_rejection(InMemoryGraphStore::new(), Ok, schema_version, bytes)?;
    }
    Ok(())
}

#[test]
fn malformed_constraint_survives_file_restart_and_rejects_without_writes()
-> Result<(), Box<dyn std::error::Error>> {
    for (schema_version, bytes) in [(2, vec![1]), (1, vec![]), (1, vec![0])] {
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("graph.snapshot");
        let mut committed_bytes = Vec::new();
        require_rejection(
            FileGraphStore::open(&path)?,
            |store| {
                drop(store);
                committed_bytes =
                    std::fs::read(&path).map_err(|error| StoreError::Backend(error.to_string()))?;
                FileGraphStore::open(&path)
            },
            schema_version,
            bytes,
        )?;
        if std::fs::read(&path)? != committed_bytes {
            return Err(
                std::io::Error::other("rejected re-resolution changed snapshot bytes").into(),
            );
        }
    }
    Ok(())
}

fn require_rejection<S: GraphStore>(
    store: S,
    reopen: impl FnOnce(S) -> Result<S, StoreError>,
    schema_version: u32,
    bytes: Vec<u8>,
) -> Result<(), IndexError> {
    let mut initial_indexer = Indexer::new(
        store,
        trait_tests::TraitFixture,
        RepositoryId::derive(&[b"constraint-fixture"]),
        WorktreeId::derive(&[b"constraint-worktree"]),
    );
    let first = GenerationId::derive(&[b"constraint-base"]);
    let files = [
        trait_tests::source("implementation.fixture", "implementation"),
        trait_tests::source("target.fixture", "function"),
    ];
    let mut delta = initial_indexer.prepare_delta(&files, IndexRunId::derive(&[b"base"]), first)?;
    let occurrence = delta
        .upsert_nodes
        .iter_mut()
        .find(|node| matches!(node.kind, NodeKind::UnresolvedReference { .. }))
        .ok_or_else(|| {
            IndexError::Store(StoreError::Integrity(
                "fixture occurrence missing".to_owned(),
            ))
        })?;
    occurrence.extension_payload = Some(ExtensionPayload {
        namespace: syntaxmesh_language_sdk::REFERENCE_RESOLUTION_NAMESPACE.to_owned(),
        schema_version,
        bytes,
    });
    initial_indexer.store_mut().apply_delta(delta)?;
    let retained = initial_indexer.store().historical_snapshot(first)?;
    let history = initial_indexer.store().generation_history()?;
    let (persisted_store, extractor, repository, worktree) = initial_indexer.into_parts();
    let mut indexer = Indexer::new(reopen(persisted_store)?, extractor, repository, worktree);
    if indexer.store().historical_snapshot(first)? != retained
        || indexer.store().generation_history()? != history
    {
        return Err(IndexError::Store(StoreError::Integrity(
            "restart changed seeded metadata".to_owned(),
        )));
    }
    let next = GenerationId::derive(&[b"constraint-next"]);
    let result = indexer.index(
        &[
            files.first().ok_or(IndexError::NoFiles)?.clone(),
            trait_tests::source("target.fixture", "trait"),
        ],
        IndexRunId::derive(&[b"next"]),
        next,
    );
    if !matches!(result, Err(IndexError::Store(StoreError::Integrity(ref message)))
            if message == "invalid persisted reference resolution constraint")
        || indexer.store().historical_snapshot(first)? != retained
        || indexer.store().generation_history()? != history
    {
        return Err(IndexError::Store(StoreError::Integrity(
            "malformed constraint did not fail before publication".to_owned(),
        )));
    }
    Ok(())
}
