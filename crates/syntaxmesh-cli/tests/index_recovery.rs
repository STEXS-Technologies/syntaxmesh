use std::error::Error;
use std::fs;
use std::process::Command;

use syntaxmesh_store::{DurableRecordStore, FileGraphStore};
use syntaxmesh_store_turso::TursoGraphStore;

#[test]
fn unchanged_index_and_watch_fail_closed_on_invalid_publication_records()
-> Result<(), Box<dyn Error>> {
    for turso in [false, true] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        fs::create_dir(&root)?;
        fs::write(
            root.join("lib.rs"),
            "pub fn retained_recovery_symbol() {}\n",
        )?;
        let target = fixture
            .path()
            .join(if turso { "graph.db" } else { "graph.snapshot" });
        if turso {
            TursoGraphStore::migrate(&target)?;
        }
        let command = if turso { "index-turso" } else { "index" };
        let baseline = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(command)
            .arg(&root)
            .arg(&target)
            .arg("--verify")
            .output()?;
        if !baseline.status.success() {
            return Err("baseline indexing failed".into());
        }
        let key = "syntaxmesh.penelope.index.v1/invalid-recovery-fixture";
        let invalid = b"invalid publication record";
        if turso {
            let mut store = TursoGraphStore::open(&target)?;
            store.compare_exchange_record(key, None, invalid)?;
        } else {
            let mut store = FileGraphStore::open(&target)?;
            store.compare_exchange_record(key, None, invalid)?;
        }
        for operation in [command, if turso { "watch-turso" } else { "watch" }] {
            let mut process = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
            process
                .arg(operation)
                .arg(&root)
                .arg(&target)
                .arg("--verify");
            if operation.starts_with("watch") {
                process.args(["--watch-duration-ms", "10"]);
            }
            let rejected = process.output()?;
            if rejected.status.success()
                || !rejected.stdout.is_empty()
                || !String::from_utf8(rejected.stderr)?.contains("engine error")
            {
                return Err("unchanged source bypassed publication recovery".into());
            }
        }
        let retained = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(if turso { "search-turso" } else { "search" })
            .arg(&target)
            .arg("retained_recovery_symbol")
            .output()?;
        if !retained.status.success()
            || !String::from_utf8(retained.stdout)?.contains("retained_recovery_symbol")
        {
            return Err("failed recovery damaged published source facts".into());
        }
    }
    Ok(())
}
