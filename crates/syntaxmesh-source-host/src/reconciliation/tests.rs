use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_store::InMemoryGraphStore;

#[path = "input_stability/tests.rs"]
mod input_stability;

struct CountingRust(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl LanguageExtractor for CountingRust {
    fn language(&self) -> &'static str {
        "rust"
    }
    fn producer_identity(&self) -> syntaxmesh_language_sdk::ExtractorIdentity {
        RustExtractor.producer_identity()
    }
    fn extract(
        &self,
        source: &syntaxmesh_language_sdk::SourceFile,
    ) -> Result<syntaxmesh_language_sdk::Extraction, syntaxmesh_language_sdk::ExtractorError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        RustExtractor.extract(source)
    }
}

#[test]
fn partial_policy_retries_failed_sources_without_accepting_an_attempt_generation()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let (repository, worktree) = crate::repository_scope(&root);
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        CountingRust(attempts.clone()),
        repository,
        worktree,
    )
    .with_statechronicle_verification();
    let fingerprint = RustExtractor.configuration_fingerprint();
    std::fs::write(root.join("input.rs"), "fn original() {}")?;
    let strict =
        super::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, &[])?;
    engine =
        engine.with_source_syntax_policy(syntaxmesh_engine::SourceSyntaxPolicy::RecordFailures);
    let backfill =
        super::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, &[])?;
    if !backfill.published || strict.generation == backfill.generation {
        return Err("policy change reused strict-only planning identity".into());
    }
    std::fs::write(root.join("input.rs"), "fn fixture_function_{NAME}() {}")?;
    let failure =
        super::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, &[])?;
    let retry =
        super::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, &[])?;
    if !failure.published
        || retry.published
        || retry.generation != failure.generation
        || attempts.load(std::sync::atomic::Ordering::SeqCst) != 4
        || engine
            .source_processing_coverage_at(retry.generation)?
            .syntax_failed
            .len()
            != 1
        || engine.verify_statechronicle_history()?.is_none()
    {
        return Err("partial host retry skipped extraction or fabricated publication".into());
    }
    Ok(())
}

#[test]
fn extended_tsconfig_only_edit_reconciles_after_provider_restart()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    std::fs::write(
        root.join("syntaxmesh.toml"),
        "[module_resolution]\nprofile = \"node\"\n",
    )?;
    std::fs::write(root.join("tsconfig.json"), r#"{"extends":"./base.json"}"#)?;
    std::fs::write(
        root.join("base.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@chosen":["./before.ts"]}}}"#,
    )?;
    std::fs::write(
        root.join("caller.ts"),
        "import { chosen } from '@chosen'; chosen();",
    )?;
    std::fs::write(root.join("before.ts"), "export function chosen() {}")?;
    std::fs::write(root.join("after.ts"), "export function chosen() {}")?;
    let config = crate::ProjectConfig::load(&root)?;
    let mut initial =
        crate::configured_source_engine(InMemoryGraphStore::new(), &root, &config, true)?;
    let first = crate::reconcile_structural_sources(
        &mut initial.engine,
        &root,
        &["ts"],
        &initial.extractor_fingerprint,
        &initial.resolver_fingerprint,
    )?;
    let historical = initial
        .engine
        .query(first.generation)
        .graph_at(first.generation)?;
    let store = initial.engine.into_store();
    // Replace the provider to ensure dependency coverage comes from durable records.
    let mut setup = crate::configured_source_engine(store, &root, &config, true)?;
    let unchanged_restart = crate::reconcile_structural_sources(
        &mut setup.engine,
        &root,
        &["ts"],
        &setup.extractor_fingerprint,
        &setup.resolver_fingerprint,
    )?;
    if unchanged_restart.published {
        return Err("restart republished unchanged resolver inputs".into());
    }
    std::fs::write(
        root.join("base.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@chosen":["./after.ts"]}}}"#,
    )?;
    let changed = crate::reconcile_structural_sources(
        &mut setup.engine,
        &root,
        &["ts"],
        &setup.extractor_fingerprint,
        &setup.resolver_fingerprint,
    )?;
    let current = setup
        .engine
        .query(changed.generation)
        .graph_at(changed.generation)?;
    let before_edges = historical
        .edges
        .iter()
        .filter(|edge| edge.relation == syntaxmesh_core::RelationKind::ResolvesTo)
        .collect::<Vec<_>>();
    let after_edges = current
        .edges
        .iter()
        .filter(|edge| edge.relation == syntaxmesh_core::RelationKind::ResolvesTo)
        .collect::<Vec<_>>();
    if !changed.published
        || before_edges.is_empty()
        || before_edges == after_edges
        || setup
            .engine
            .query(first.generation)
            .graph_at(first.generation)?
            != historical
        || setup.engine.verify_statechronicle_history()?.is_none()
    {
        return Err("config-only edit did not update resolution while retaining history".into());
    }
    let repeated = crate::reconcile_structural_sources(
        &mut setup.engine,
        &root,
        &["ts"],
        &setup.extractor_fingerprint,
        &setup.resolver_fingerprint,
    )?;
    if repeated.published {
        return Err("config-only transition was not idempotent".into());
    }
    Ok(())
}

#[test]
fn repeated_passes_skip_publication_and_reverts_keep_distinct_history()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let (repository, worktree) = crate::repository_scope(&root);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    )
    .with_statechronicle_verification();
    let fingerprint = RustExtractor.configuration_fingerprint();
    let mut generations = Vec::new();
    for content in [
        "pub fn before() {}",
        "pub fn after() {}",
        "pub fn before() {}",
    ] {
        std::fs::write(root.join("lib.rs"), content)?;
        let pass =
            super::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, &[])?;
        let repeat =
            super::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, &[])?;
        if !pass.published
            || repeat.published
            || pass.generation != repeat.generation
            || generations.contains(&pass.generation)
        {
            return Err("reconciliation lost publication/no-op/revert identity".into());
        }
        generations.push(pass.generation);
    }
    if engine.verify_statechronicle_history()?.is_none() {
        return Err("reconciliation lost verification history".into());
    }
    Ok(())
}
#[test]
fn static_resolver_revision_republishes_once_without_source_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    std::fs::write(
        root.join("lib.rs"),
        "pub fn caller() { wrong::helper(); }\nmod actual { pub fn helper() {} }\n",
    )?;
    let mut setup = crate::configured_source_engine(
        syntaxmesh_store::InMemoryGraphStore::new(),
        &root,
        &crate::ProjectConfig::default(),
        false,
    )?;
    let old = crate::reconcile_structural_sources(
        &mut setup.engine,
        &root,
        syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS,
        &setup.extractor_fingerprint,
        &[],
    )?;
    let upgraded = crate::reconcile_structural_sources(
        &mut setup.engine,
        &root,
        syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS,
        &setup.extractor_fingerprint,
        &setup.resolver_fingerprint,
    )?;
    let repeated = crate::reconcile_structural_sources(
        &mut setup.engine,
        &root,
        syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS,
        &setup.extractor_fingerprint,
        &setup.resolver_fingerprint,
    )?;
    if !old.published || !upgraded.published || repeated.published {
        return Err("static resolver revision did not invalidate once".into());
    }
    Ok(())
}
