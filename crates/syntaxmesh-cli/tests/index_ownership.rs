use std::error::Error;
use std::fs::{self, OpenOptions};
use std::process::Command;

use syntaxmesh_ownership_host::{OwnershipError, WriterLease};

#[test]
fn unattached_turso_reads_reject_owner_before_opening_database() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let database = fixture.path().join("graph.db");
    fs::write(&database, b"deliberately invalid database")?;
    let before = fs::read(&database)?;
    let id = "1".repeat(64);
    let cases = [
        ("history-turso", vec![id.as_str()]),
        ("fact-history-turso", vec!["node", &id]),
        ("fact-lineage-turso", vec!["node", &id]),
        ("graph-at-turso", vec![id.as_str()]),
        ("graph-at-known-by-turso", vec![&id, "1"]),
        ("changes-turso", vec![&id, &id]),
        ("change-set-at-turso", vec![&id, &id]),
        ("change-set-events-turso", vec![&id, &id, "1"]),
        ("change-correlations-turso", vec![&id, "node", &id, "1"]),
        ("accepted-between-turso", vec!["0", "1", "1"]),
        ("observed-between-turso", vec!["0", "1", "1"]),
        ("path-turso", vec![&id, &id, "1"]),
        ("subgraph-turso", vec![&id, "1", "1", "1"]),
        (
            "neighborhood-at-turso",
            vec![&id, &id, "1", "1", "1", "1", "1024"],
        ),
        (
            "consequence-neighborhood-turso",
            vec![&id, "event", &id, "1", "1", "1", "1"],
        ),
        (
            "consequence-trace-turso",
            vec![&id, &id, "event", &id, "1", "1", "1", "1", "1024"],
        ),
        ("impact-turso", vec!["example"]),
        ("cycles-turso", vec!["current", "calls"]),
        ("export-turso", vec![]),
        ("statechronicle-verify-turso", vec![]),
    ];
    let owner = WriterLease::acquire(&database)?;
    if owner.target() != fs::canonicalize(&database)? {
        return Err("lease target is not the canonical database".into());
    }
    for (command, arguments) in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(command)
            .arg(&database)
            .args(arguments)
            .output()?;
        if output.status.success()
            || !output.stdout.is_empty()
            || !String::from_utf8_lossy(&output.stderr).contains("already owned")
            || fs::read(&database)? != before
        {
            return Err(format!(
                "{command} bypassed ownership: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
    }
    drop(owner);
    Ok(())
}

#[cfg(unix)]
#[test]
fn embedded_lease_excludes_cli_across_parent_and_existing_store_aliases()
-> Result<(), Box<dyn Error>> {
    for (command, filename) in [("index", "graph.snapshot"), ("index-turso", "graph.db")] {
        let fixture = tempfile::tempdir()?;
        let source = fixture.path().join("source");
        fs::create_dir(&source)?;
        fs::write(source.join("lib.rs"), "pub fn embedded_owner() {}\n")?;
        let state = fixture.path().join("state");
        fs::create_dir(&state)?;
        let alias = fixture.path().join("state-alias");
        std::os::unix::fs::symlink(&state, &alias)?;
        let target = state.join(filename);
        let invoke = |path: &std::path::Path| {
            Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(command)
                .arg(&source)
                .arg(path)
                .output()
        };
        let owner = WriterLease::acquire(&target)?;
        if !matches!(
            WriterLease::acquire(&alias.join(filename)),
            Err(OwnershipError::AlreadyOwned)
        ) {
            return Err("embedded alias did not report typed competing ownership".into());
        }
        let blocked = invoke(&alias.join(filename))?;
        if blocked.status.success()
            || target.exists()
            || !String::from_utf8(blocked.stderr)?.contains("already owned")
        {
            return Err("CLI parent alias bypassed embedded ownership".into());
        }
        drop(owner);
        if command == "index-turso" {
            syntaxmesh_store_turso::TursoGraphStore::migrate(&target)?;
        }
        let accepted = invoke(&target)?;
        if !accepted.status.success() {
            return Err(String::from_utf8(accepted.stderr)?.into());
        }
        let store_alias = fixture.path().join("store-alias");
        std::os::unix::fs::symlink(&target, &store_alias)?;
        let before = fs::read(&target)?;
        let next_owner = WriterLease::acquire(&target)?;
        let blocked_existing = invoke(&store_alias)?;
        if blocked_existing.status.success()
            || fs::read(&target)? != before
            || !String::from_utf8(blocked_existing.stderr)?.contains("already owned")
        {
            return Err("CLI existing-store alias bypassed embedded ownership".into());
        }
        drop(next_owner);
        let released = invoke(&store_alias)?;
        if !released.status.success() {
            return Err(String::from_utf8(released.stderr)?.into());
        }
    }
    Ok(())
}

#[test]
fn migration_obeys_existing_writer_lease() -> Result<(), Box<dyn Error>> {
    for command in ["sqlite-migrate", "turso-migrate"] {
        let fixture = tempfile::tempdir()?;
        let target = fixture.path().join("graph.db");
        let sidecar = fixture.path().join("graph.db.syntaxmesh-owner.lock");
        let owner = WriterLease::acquire(&target)?;
        let invoke = || {
            Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(command)
                .arg(&target)
                .output()
        };
        let blocked = invoke()?;
        if blocked.status.success()
            || target.exists()
            || !String::from_utf8(blocked.stderr)?.contains("already owned")
        {
            return Err("migration bypassed writer ownership".into());
        }
        drop(owner);
        let accepted = invoke()?;
        if !accepted.status.success() || !target.is_file() || !sidecar.is_file() {
            return Err(format!(
                "migration after release failed: {}",
                String::from_utf8(accepted.stderr)?
            )
            .into());
        }
    }
    Ok(())
}

#[test]
fn competing_process_is_rejected_before_store_open_and_release_allows_indexing()
-> Result<(), Box<dyn Error>> {
    for (command, filename) in [("index", "graph.snapshot"), ("index-turso", "graph.db")] {
        let fixture = tempfile::tempdir()?;
        let source = fixture.path().join("source");
        fs::create_dir(&source)?;
        fs::write(source.join("lib.rs"), "pub fn ownership_fixture() {}\n")?;
        let target = fixture.path().join(filename);
        let sidecar = fixture
            .path()
            .join(format!("{filename}.syntaxmesh-owner.lock"));
        let owner = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&sidecar)?;
        owner.try_lock()?;
        let blocked = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(command)
            .arg(&source)
            .arg(&target)
            .output()?;
        if blocked.status.success()
            || target.exists()
            || !String::from_utf8(blocked.stderr)?
                .contains("already owned by another cooperating writer")
        {
            return Err("competing CLI process did not fail before store open".into());
        }
        drop(owner);
        if command == "index-turso" {
            syntaxmesh_store_turso::TursoGraphStore::migrate(&target)?;
        }
        let accepted = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(command)
            .arg(&source)
            .arg(&target)
            .output()?;
        if !accepted.status.success() || !target.is_file() || !sidecar.is_file() {
            return Err(format!(
                "released ownership did not allow indexing: {}",
                String::from_utf8(accepted.stderr)?
            )
            .into());
        }
    }
    Ok(())
}
