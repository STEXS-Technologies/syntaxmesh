use super::*;
use syntaxmesh_core::FileVersion;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_store::InMemoryGraphStore;

fn require(condition: bool) -> Result<(), IndexError> {
    if condition {
        Ok(())
    } else {
        Err(IndexError::Store(StoreError::Integrity(
            "processing coverage regression".to_owned(),
        )))
    }
}

fn source(path: &str, content: &str) -> Result<SourceFile, IndexError> {
    Ok(SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[path.as_bytes()]),
            normalized_path: path.to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len()).map_err(|error| {
                IndexError::Extractor(ExtractorError::InvalidInput(error.to_string()))
            })?,
        },
        content: content.to_owned(),
    })
}

#[test]
fn strict_rejects_mixed_batch_without_publication() -> Result<(), IndexError> {
    let mut indexer = Indexer::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        RepositoryId::derive(&[b"strict"]),
        WorktreeId::derive(&[b"strict"]),
    );
    let result = indexer.index(
        &[
            source("valid.rs", "fn valid() {}")?,
            source("invalid.rs", "fn broken(")?,
        ],
        IndexRunId::derive(&[b"strict"]),
        GenerationId::derive(&[b"strict"]),
    );
    require(matches!(
        result,
        Err(IndexError::Extractor(ExtractorError::SyntaxError(_)))
    ))?;
    require(indexer.store().generation_history()?.is_empty())?;
    Ok(())
}

#[test]
fn partial_retracts_invalid_file_facts_and_preserves_history_through_repair()
-> Result<(), IndexError> {
    let mut indexer = Indexer::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        RepositoryId::derive(&[b"partial"]),
        WorktreeId::derive(&[b"partial"]),
    )
    .with_source_syntax_policy(SourceSyntaxPolicy::RecordFailures);
    let first = GenerationId::derive(&[b"first"]);
    indexer.index(
        &[source("input.rs", "fn original() {}")?],
        IndexRunId::derive(&[b"first"]),
        first,
    )?;
    let original = indexer.store().historical_snapshot(first)?;
    let failed = GenerationId::derive(&[b"failed"]);
    indexer.index(
        &[
            source("input.rs", "fn broken(")?,
            source("valid.rs", "fn valid() {}")?,
        ],
        IndexRunId::derive(&[b"failed"]),
        failed,
    )?;
    let partial = indexer.store().historical_snapshot(failed)?;
    require(!partial.nodes.iter().any(|node| node.name == "original"))?;
    require(partial.nodes.iter().any(|node| node.name == "valid"))?;
    require(partial.nodes.iter().any(|node| matches!(&node.kind, NodeKind::External { namespace, kind } if namespace == syntaxmesh_language_sdk::SOURCE_PROCESSING_NAMESPACE && kind == "syntax_failed.v1")))?;
    require(indexer.store().historical_snapshot(first)? == original)?;
    let retry = indexer.prepare_delta(
        &[
            source("input.rs", "fn broken(")?,
            source("valid.rs", "fn valid() {}")?,
        ],
        IndexRunId::derive(&[b"retry"]),
        GenerationId::derive(&[b"retry"]),
    )?;
    require(
        retry.changed_files.is_empty()
            && retry.upsert_nodes.is_empty()
            && retry.upsert_provenance.is_empty()
            && retry.remove_nodes.is_empty()
            && retry.upsert_edges.is_empty()
            && retry.remove_edges.is_empty()
            && retry.removed_files.is_empty(),
    )?;
    let repaired = GenerationId::derive(&[b"repaired"]);
    indexer.index(
        &[
            source("input.rs", "fn repaired() {}")?,
            source("valid.rs", "fn valid() {}")?,
        ],
        IndexRunId::derive(&[b"repaired"]),
        repaired,
    )?;
    let current = indexer.store().historical_snapshot(repaired)?;
    require(current.nodes.iter().any(|node| node.name == "repaired"))?;
    require(!current.nodes.iter().any(
        |node| matches!(&node.kind, NodeKind::External { kind, .. } if kind == "syntax_failed.v1"),
    ))?;
    require(indexer.store().historical_snapshot(failed)? == partial)?;
    Ok(())
}

#[test]
fn enabling_coverage_backfills_unchanged_legacy_sources() -> Result<(), IndexError> {
    let mut indexer = Indexer::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        RepositoryId::derive(&[b"legacy"]),
        WorktreeId::derive(&[b"legacy"]),
    );
    let files = [source("legacy.rs", "fn legacy() {}")?];
    let first = GenerationId::derive(&[b"legacy-first"]);
    indexer.index(&files, IndexRunId::derive(&[b"legacy-first"]), first)?;
    let retained = indexer.store().historical_snapshot(first)?;
    indexer = indexer.with_source_syntax_policy(SourceSyntaxPolicy::RecordFailures);
    let next = GenerationId::derive(&[b"legacy-coverage"]);
    indexer.index(&files, IndexRunId::derive(&[b"legacy-coverage"]), next)?;
    let snapshot = indexer.store().historical_snapshot(next)?;
    require(snapshot.nodes.iter().any(|node| matches!(&node.kind, NodeKind::External { namespace, kind } if namespace == syntaxmesh_language_sdk::SOURCE_PROCESSING_NAMESPACE && kind == "completed.v1")))?;
    require(indexer.store().historical_snapshot(first)? == retained)?;
    let unchanged = indexer.prepare_delta(
        &files,
        IndexRunId::derive(&[b"legacy-unchanged"]),
        GenerationId::derive(&[b"legacy-unchanged"]),
    )?;
    require(
        unchanged.changed_files.is_empty()
            && unchanged.upsert_nodes.is_empty()
            && unchanged.remove_nodes.is_empty(),
    )?;
    Ok(())
}
