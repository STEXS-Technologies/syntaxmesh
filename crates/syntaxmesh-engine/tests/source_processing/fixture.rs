use std::error::Error;
use syntaxmesh_core::{
    FileId, FileVersion, GenerationId, IndexRunId, RelationKind, RepositoryId, WorktreeId,
};
use syntaxmesh_engine::{SourceSyntaxPolicy, SyntaxMeshEngine};
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::{
    Extraction, ExtractorError, ExtractorIdentity, LanguageExtractor, SourceFile, SourceProcessing,
};
use syntaxmesh_store::{DurableRecordStore, GraphStore};

struct RejectInternal;

struct UpgradedRust;

impl LanguageExtractor for UpgradedRust {
    fn language(&self) -> &'static str {
        "rust"
    }
    fn producer_identity(&self) -> ExtractorIdentity {
        ExtractorIdentity::new("syntaxmesh.lang.rust", "fixture-upgrade")
    }
    fn extract(&self, source: &SourceFile) -> Result<Extraction, ExtractorError> {
        let mut extraction = RustExtractor.extract(source)?;
        extraction.provenance.producer_version = "fixture-upgrade".to_owned();
        Ok(extraction)
    }
}

impl LanguageExtractor for RejectInternal {
    fn language(&self) -> &'static str {
        "rust"
    }
    fn producer_identity(&self) -> ExtractorIdentity {
        RustExtractor.producer_identity()
    }
    fn extract(&self, _source: &SourceFile) -> Result<Extraction, ExtractorError> {
        Err(ExtractorError::InvalidInput(
            "fixture internal invariant failure".to_owned(),
        ))
    }
}

fn source(path: &str, content: &str) -> Result<SourceFile, Box<dyn Error>> {
    Ok(SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[path.as_bytes()]),
            normalized_path: path.to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len())?,
        },
        content: content.to_owned(),
    })
}

pub(super) fn exercise<S: GraphStore + DurableRecordStore>(
    open: impl Fn() -> Result<S, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let repository = RepositoryId::derive(&[b"partial-processing"]);
    let worktree = WorktreeId::derive(&[b"partial-processing"]);
    let first = GenerationId::derive(&[b"partial-first"]);
    let failed = GenerationId::derive(&[b"partial-failed"]);
    let repaired = GenerationId::derive(&[b"partial-repaired"]);
    let mut engine = SyntaxMeshEngine::new(open()?, RustExtractor, repository, worktree)
        .with_statechronicle_verification();
    engine.index(
        &[
            source("input.rs", "fn original() {}")?,
            source("valid.rs", "fn valid() { original(); }")?,
        ],
        IndexRunId::derive(&[b"partial-first"]),
        first,
    )?;
    let original = engine.into_store().historical_snapshot(first)?;
    let target = original
        .nodes
        .iter()
        .find(|node| node.name == "original")
        .ok_or("initial target missing")?
        .id;
    if !original
        .edges
        .iter()
        .any(|edge| edge.target == target && edge.relation == RelationKind::Calls)
    {
        return Err("initial sibling call did not resolve".into());
    }
    let inputs = [
        source("input.rs", "fn fixture_function_{NAME}() {}")?,
        source("valid.rs", "fn valid() { original(); }")?,
    ];
    let mut updater = SyntaxMeshEngine::new(open()?, RustExtractor, repository, worktree)
        .with_source_syntax_policy(SourceSyntaxPolicy::RecordFailures)
        .with_statechronicle_verification();
    updater.index(&inputs, IndexRunId::derive(&[b"partial-failed"]), failed)?;
    drop(updater);
    let store = open()?;
    let partial = store.historical_snapshot(failed)?;
    if store.historical_snapshot(first)? != original
        || partial.nodes.iter().any(|node| node.id == target)
        || !partial.nodes.iter().any(|node| node.name == "valid")
        || partial.edges.iter().any(|edge| edge.target == target)
    {
        return Err(
            "partial publication lost history or retained stale current declarations".into(),
        );
    }
    let file = &inputs[0].file;
    let mut found_failure = false;
    for node in partial
        .nodes
        .iter()
        .filter(|node| node.owner_file == Some(file.file_id))
    {
        let provenance = partial
            .provenance
            .iter()
            .find(|item| item.id == node.provenance)
            .ok_or("processing provenance missing after restart")?;
        if matches!(
            SourceProcessing::from_facts(file, node, provenance)?,
            Some(SourceProcessing::SyntaxFailed(_))
        ) {
            found_failure = true;
        }
    }
    if !found_failure {
        return Err("failed file lacks canonical processing evidence after restart".into());
    }
    let history = store.generation_history()?;
    drop(store);
    let mut internal = SyntaxMeshEngine::new(open()?, RejectInternal, repository, worktree)
        .with_source_syntax_policy(SourceSyntaxPolicy::RecordFailures)
        .with_statechronicle_verification();
    let rejection = internal.index(
        &inputs,
        IndexRunId::derive(&[b"internal-rejection"]),
        GenerationId::derive(&[b"internal-rejection"]),
    );
    match rejection {
        Err(syntaxmesh_engine::EngineError::Index(error))
            if format!("{error:?}").starts_with("Extractor(InvalidInput(") => {}
        _ => return Err("partial policy tolerated an internal extractor failure".into()),
    }
    drop(internal);
    let rejected_store = open()?;
    if rejected_store.historical_snapshot(failed)? != partial
        || rejected_store.generation_history()? != history
    {
        return Err("internal extraction rejection changed durable graph or history".into());
    }
    let mut upgraded = SyntaxMeshEngine::new(rejected_store, UpgradedRust, repository, worktree)
        .with_source_syntax_policy(SourceSyntaxPolicy::RecordFailures)
        .with_statechronicle_verification();
    let upgrade_delta = upgraded.prepare_source_delta(
        &inputs,
        IndexRunId::derive(&[b"partial-upgrade"]),
        GenerationId::derive(&[b"partial-upgrade"]),
    )?;
    let previous_failure = partial
        .nodes
        .iter()
        .find(|node| node.owner_file == Some(file.file_id))
        .ok_or("prior failed-file fact missing")?;
    let changed_failure = upgrade_delta
        .upsert_nodes
        .iter()
        .find(|node| node.id == previous_failure.id)
        .ok_or("producer upgrade skipped unchanged failed bytes")?;
    let changed_evidence = upgrade_delta
        .upsert_provenance
        .iter()
        .find(|item| item.id == changed_failure.provenance)
        .ok_or("upgraded processing provenance missing")?;
    if changed_failure.provenance == previous_failure.provenance
        || changed_evidence.producer_version != "fixture-upgrade"
        || !matches!(
            SourceProcessing::from_facts(file, changed_failure, changed_evidence)?,
            Some(SourceProcessing::SyntaxFailed(_))
        )
    {
        return Err("producer upgrade failed to retain typed failure with new provenance".into());
    }
    let upgrade_store = upgraded.into_store();
    if upgrade_store.historical_snapshot(failed)? != partial
        || upgrade_store.generation_history()? != history
    {
        return Err("upgrade preparation changed accepted graph/history".into());
    }
    let mut recovery = SyntaxMeshEngine::new(upgrade_store, RustExtractor, repository, worktree)
        .with_source_syntax_policy(SourceSyntaxPolicy::RecordFailures)
        .with_statechronicle_verification();
    let current_coverage = recovery.source_processing_coverage_at(failed)?;
    let legacy_coverage = recovery.source_processing_coverage_at(first)?;
    if current_coverage.generation != failed
        || current_coverage.completed.len() != 1
        || current_coverage.syntax_failed.len() != 1
        || !current_coverage.unclassified.is_empty()
        || legacy_coverage.unclassified.len() != 2
        || !legacy_coverage.completed.is_empty()
        || !legacy_coverage.syntax_failed.is_empty()
    {
        return Err("processing coverage mislabeled partial or legacy history".into());
    }
    if recovery.verify_statechronicle_history()?.is_none() {
        return Err("partial publication lost verified history".into());
    }
    let retry = recovery.prepare_source_delta(
        &inputs,
        IndexRunId::derive(&[b"partial-retry"]),
        GenerationId::derive(&[b"partial-retry"]),
    )?;
    if !retry.changed_files.is_empty()
        || !retry.upsert_nodes.is_empty()
        || !retry.remove_nodes.is_empty()
        || !retry.upsert_provenance.is_empty()
    {
        return Err("unchanged failure generated new facts".into());
    }
    recovery.index(
        &[
            source("input.rs", "fn repaired() {}")?,
            source("valid.rs", "fn valid() { original(); }")?,
        ],
        IndexRunId::derive(&[b"partial-repaired"]),
        repaired,
    )?;
    drop(recovery);
    let repaired_store = open()?;
    if repaired_store.historical_snapshot(failed)? != partial
        || !repaired_store
            .historical_snapshot(repaired)?
            .nodes
            .iter()
            .any(|node| node.name == "repaired")
    {
        return Err("repair lost historical failure or current repaired facts".into());
    }
    let mut verified = SyntaxMeshEngine::new(repaired_store, RustExtractor, repository, worktree);
    let repaired_coverage = verified.source_processing_coverage_at(repaired)?;
    if repaired_coverage.completed.len() != 2
        || !repaired_coverage.syntax_failed.is_empty()
        || !repaired_coverage.unclassified.is_empty()
    {
        return Err("repaired processing coverage did not become complete".into());
    }
    if verified.verify_statechronicle_history()?.is_none() {
        return Err("repair lost verified history after reopen".into());
    }
    let foreign = SyntaxMeshEngine::new(
        verified.into_store(),
        RustExtractor,
        RepositoryId::derive(&[b"foreign"]),
        worktree,
    );
    if foreign.source_processing_coverage_at(failed).is_ok() {
        return Err("processing coverage accepted a foreign scope".into());
    }
    Ok(())
}
