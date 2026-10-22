use std::error::Error;
use std::process::Command;
use syntaxmesh_store::GraphStore;
use syntaxmesh_store_turso::TursoGraphStore;

#[test]
fn verified_partial_daemon_publishes_healthy_sources_and_recovers_repaired_file()
-> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("good.rs"), "pub fn healthy_source() {}")?;
    std::fs::write(root.join("bad.rs"), "pub fn repaired_source( {")?;
    let database = temporary.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let run = |partial: bool| -> Result<bool, Box<dyn Error>> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmeshd"));
        command.arg(&root).arg(&database).arg("127.0.0.1:0").args([
            "--verify",
            "--poll-only",
            "--reconcile-ms",
            "100",
            "--watch-duration-ms",
            "300",
        ]);
        if partial {
            command.arg("--record-syntax-failures");
        }
        Ok(command.output()?.status.success())
    };
    if run(false)? || !run(true)? {
        return Err("daemon strict/partial syntax policy mismatch".into());
    }
    let (repository, worktree) = syntaxmesh_source_host::repository_scope(&root);
    let store = TursoGraphStore::open(&database)?;
    let retained = store
        .current_generation(repository, worktree)?
        .ok_or("missing partial generation")?
        .generation;
    let before = store.historical_snapshot(retained)?;
    if !before
        .nodes
        .iter()
        .any(|node| node.name == "healthy_source")
        || before
            .nodes
            .iter()
            .any(|node| node.name == "repaired_source")
    {
        return Err("partial daemon lost healthy facts or published failed extraction".into());
    }
    drop(store);
    std::fs::write(root.join("bad.rs"), "pub fn repaired_source() {}")?;
    if !run(true)? {
        return Err("daemon failed to recover repaired source".into());
    }
    let repaired_store = TursoGraphStore::open(&database)?;
    let current = repaired_store
        .current_generation(repository, worktree)?
        .ok_or("missing repaired generation")?
        .generation;
    if current == retained
        || !repaired_store
            .historical_snapshot(current)?
            .nodes
            .iter()
            .any(|node| node.name == "repaired_source")
        || repaired_store.historical_snapshot(retained)? != before
    {
        return Err("repair failed to publish independently of retained partial history".into());
    }
    Ok(())
}
