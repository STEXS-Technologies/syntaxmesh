//! Reuse the Engine's exact producer upgrade/history scenario on Turso WAL.

#[path = "../../syntaxmesh-engine/tests/rust_definition_upgrade/fixture.rs"]
mod fixture;

#[test]
fn syntax_failure_and_repair_preserve_verified_turso_history()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&path)?;
    fixture::exercise_syntax_failure(|| Ok(syntaxmesh_store_turso::TursoGraphStore::open(&path)?))
}

#[test]
fn legacy_role_codec_upgrade_retains_verified_turso_history()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&path)?;
    fixture::exercise_legacy_role(|| Ok(syntaxmesh_store_turso::TursoGraphStore::open(&path)?))
}

#[test]
fn producer_upgrade_and_test_role_history_survive_turso_restart()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&path)?;
    fixture::exercise(|| Ok(syntaxmesh_store_turso::TursoGraphStore::open(&path)?))
}
