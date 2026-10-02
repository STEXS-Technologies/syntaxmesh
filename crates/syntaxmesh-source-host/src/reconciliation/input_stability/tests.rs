use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_resolver::{
    ModuleResolutionOutcome, ModuleResolutionProvider, ModuleResolutionRequest,
    ModuleResolverIdentity,
};
use syntaxmesh_store::InMemoryGraphStore;

struct ChangingProvider {
    identity: ModuleResolverIdentity,
    input: String,
    mutate: AtomicBool,
}

impl ModuleResolutionProvider for ChangingProvider {
    fn identity(&self) -> &ModuleResolverIdentity {
        &self.identity
    }
    fn observed_inputs(&self) -> Result<Vec<String>, String> {
        Ok(vec![self.input.clone()])
    }
    fn refresh(&self) -> Result<(), String> {
        if self.mutate.load(Ordering::SeqCst) {
            std::fs::write(&self.input, "changed during preparation")
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    fn supports(&self, _request: &ModuleResolutionRequest) -> bool {
        false
    }
    fn resolve(&self, _request: &ModuleResolutionRequest) -> ModuleResolutionOutcome {
        ModuleResolutionOutcome::Unresolved("unsupported".to_owned())
    }
}

#[test]
fn input_mutation_rejects_candidate_without_accepting_planning_marker()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().canonicalize()?;
    let input = root
        .join("policy.json")
        .into_os_string()
        .into_string()
        .map_err(|path| std::io::Error::other(format!("invalid fixture path: {path:?}")))?;
    std::fs::write(&input, "stable")?;
    std::fs::write(root.join("lib.rs"), "pub fn before() {}")?;
    let provider = Arc::new(ChangingProvider {
        identity: ModuleResolverIdentity {
            namespace: "test/input-stability".to_owned(),
            version: "1".to_owned(),
            settings_fingerprint: Vec::new(),
        },
        input,
        mutate: AtomicBool::new(false),
    });
    let (repository, worktree) = crate::repository_scope(&root);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    )
    .with_module_resolution_provider(provider.clone())
    .with_statechronicle_verification();
    let fingerprint = RustExtractor.configuration_fingerprint();
    let accepted =
        crate::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, b"test")?;
    let historical = engine
        .query(accepted.generation)
        .graph_at(accepted.generation)?;
    provider.mutate.store(true, Ordering::SeqCst);
    std::fs::write(root.join("lib.rs"), "pub fn after() {}")?;
    let rejected =
        crate::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, b"test");
    if rejected.is_ok()
        || engine
            .current_generation(repository, worktree)?
            .map(|manifest| manifest.generation)
            != Some(accepted.generation)
        || engine
            .query(accepted.generation)
            .graph_at(accepted.generation)?
            != historical
    {
        return Err("unstable input accepted a candidate or changed retained state".into());
    }
    provider.mutate.store(false, Ordering::SeqCst);
    let repaired =
        crate::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, b"test")?;
    let repeated =
        crate::reconcile_structural_sources(&mut engine, &root, &["rs"], &fingerprint, b"test")?;
    if !repaired.published
        || repeated.published
        || repaired.generation == accepted.generation
        || engine.verify_statechronicle_history()?.is_none()
    {
        return Err("rejected planning intent blocked repair or lost verified history".into());
    }
    Ok(())
}
