use syntaxmesh_scanner::SUPPORTED_SOURCE_EXTENSIONS;
use syntaxmesh_store::InMemoryGraphStore;

#[test]
fn project_policy_and_positive_override_enable_verified_mixed_source_setup()
-> Result<(), Box<dyn std::error::Error>> {
    for (project_verify, override_verify, expected) in [
        (false, false, false),
        (false, true, true),
        (true, false, true),
    ] {
        let directory = tempfile::tempdir()?;
        let root = directory.path().canonicalize()?;
        crate::init_project(&root, project_verify, &[])?;
        std::fs::write(root.join("lib.rs"), "pub fn rust_source() {}")?;
        std::fs::write(
            root.join("design.md"),
            "# Architecture\nRust owns execution.\n",
        )?;
        let config = crate::ProjectConfig::load(&root)?;
        let mut setup = super::configured_source_engine(
            InMemoryGraphStore::new(),
            &root,
            &config,
            override_verify,
        )?;
        let pass = crate::reconcile_structural_sources(
            &mut setup.engine,
            &root,
            SUPPORTED_SOURCE_EXTENSIONS,
            &setup.extractor_fingerprint,
            &setup.resolver_fingerprint,
        )?;
        if !pass.published || setup.engine.verify_statechronicle_history()?.is_some() != expected {
            return Err("configured source setup changed verification policy".into());
        }
    }
    Ok(())
}
