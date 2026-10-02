#[path = "source_processing/fixture.rs"]
mod fixture;

#[test]
fn partial_processing_retains_verified_file_history() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.snapshot");
    fixture::exercise(|| Ok(syntaxmesh_store::FileGraphStore::open(&path)?))
}
