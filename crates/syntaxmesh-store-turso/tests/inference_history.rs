//! Reuse the same authoritative premise lifecycle across SQL adapters.
use std::error::Error;
use syntaxmesh_inference::{Limits, Rule};
use syntaxmesh_query::Query;
use syntaxmesh_store::GraphStore;

#[path = "../../syntaxmesh-query/src/service/inference/tests/fixture.rs"]
mod fixture;

fn exercise<S: GraphStore>(
    mut open: impl FnMut() -> Result<S, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let mut store = open()?;
    let (rule, retained, current) = fixture::fixture(&mut store)?;
    let expected = Query::new(&store, retained).infer(&rule, &fixture::limits(), 100)?;
    fixture::verify(&store, &rule, retained, current)?;
    drop(store);
    let mut reopened = open()?;
    if Query::new(&reopened, retained).infer(&rule, &fixture::limits(), 100)? != expected {
        return Err("SQL reopen changed retained witness identity".into());
    }
    fixture::verify(&reopened, &rule, retained, current)?;
    fixture::verify_restoration(&mut reopened, &rule, retained, current)
}

#[test]
fn sqlite_inference_retains_authoritative_versions_after_reopen() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.sqlite");
    syntaxmesh_store_sqlite::SqliteGraphStore::migrate(&path)?;
    exercise(|| Ok(syntaxmesh_store_sqlite::SqliteGraphStore::open(&path)?))
}

#[test]
fn turso_inference_retains_authoritative_versions_after_reopen() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&path)?;
    exercise(|| Ok(syntaxmesh_store_turso::TursoGraphStore::open(&path)?))
}
