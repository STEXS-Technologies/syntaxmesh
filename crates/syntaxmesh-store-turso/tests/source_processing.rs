#[path = "../../syntaxmesh-engine/tests/source_processing/fixture.rs"]
mod fixture;

#[test]
fn partial_processing_retains_verified_sqlite_history() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.sqlite");
    syntaxmesh_store_sqlite::SqliteGraphStore::migrate(&path)?;
    fixture::exercise(|| Ok(syntaxmesh_store_sqlite::SqliteGraphStore::open(&path)?))
}

#[test]
fn partial_processing_retains_verified_turso_history() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&path)?;
    fixture::exercise(|| Ok(syntaxmesh_store_turso::TursoGraphStore::open(&path)?))
}
