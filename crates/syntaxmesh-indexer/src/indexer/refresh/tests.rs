use super::*;
use syntaxmesh_store::InMemoryGraphStore;

struct RejectRefresh(ModuleResolverIdentity);

impl ModuleResolutionProvider for RejectRefresh {
    fn identity(&self) -> &ModuleResolverIdentity {
        &self.0
    }

    fn refresh(&self) -> Result<(), String> {
        Err("fixture refresh failure".to_owned())
    }

    fn resolve(&self, _request: &ModuleResolutionRequest) -> ModuleResolutionOutcome {
        ModuleResolutionOutcome::Invalid("refresh should reject before resolution".to_owned())
    }
}

#[test]
fn refresh_failure_preserves_current_and_retained_graph() -> Result<(), IndexError> {
    let mut indexer = Indexer::new(
        InMemoryGraphStore::new(),
        trait_tests::TraitFixture,
        RepositoryId::derive(&[b"refresh-repository"]),
        WorktreeId::derive(&[b"refresh-worktree"]),
    );
    let first = GenerationId::derive(&[b"refresh-base"]);
    indexer.index(
        &[trait_tests::source("target.fixture", "function")],
        IndexRunId::derive(&[b"refresh-base-run"]),
        first,
    )?;
    let retained = indexer.store().historical_snapshot(first)?;
    let history = indexer.store().generation_history()?;
    indexer.set_module_resolution_provider(Some(Arc::new(RejectRefresh(ModuleResolverIdentity {
        namespace: "fixture/refresh".to_owned(),
        version: "1".to_owned(),
        settings_fingerprint: Vec::new(),
    }))));
    let result = indexer.index(
        &[trait_tests::source("target.fixture", "trait")],
        IndexRunId::derive(&[b"refresh-next-run"]),
        GenerationId::derive(&[b"refresh-next"]),
    );
    if !matches!(result, Err(IndexError::Extractor(ExtractorError::InvalidInput(ref message)))
        if message == "module resolver refresh failed: fixture refresh failure")
        || indexer.store().historical_snapshot(first)? != retained
        || indexer.store().generation_history()? != history
    {
        return Err(IndexError::Store(StoreError::Integrity(
            "refresh failure did not preserve graph publication boundary".to_owned(),
        )));
    }
    Ok(())
}
