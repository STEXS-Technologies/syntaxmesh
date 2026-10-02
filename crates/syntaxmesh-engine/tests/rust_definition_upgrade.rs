//! File-store evidence for the shared Rust producer upgrade/role history fixture.

#[path = "rust_definition_upgrade/fixture.rs"]
mod fixture;

#[test]
fn syntax_failure_and_repair_preserve_verified_file_history()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.snapshot");
    fixture::exercise_syntax_failure(|| Ok(syntaxmesh_store::FileGraphStore::open(&path)?))
}

#[test]
fn legacy_role_codec_upgrade_retains_verified_file_history()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.snapshot");
    fixture::exercise_legacy_role(|| Ok(syntaxmesh_store::FileGraphStore::open(&path)?))
}

#[test]
fn producer_upgrade_preserves_old_evidence_after_file_restart()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("graph.snapshot");
    fixture::exercise(|| Ok(syntaxmesh_store::FileGraphStore::open(&path)?))
}
