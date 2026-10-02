use serde_json::Value;
use std::error::Error;
use std::process::Command;

#[test]
fn source_cycles_match_file_and_turso_and_survive_retraction() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir(&root)?;
    let source = root.join("lib.rs");
    std::fs::write(
        &source,
        "pub fn alpha() { beta(); }\npub fn beta() { alpha(); }\n",
    )?;
    let file = fixture.path().join("graph.snapshot");
    let database = fixture.path().join("graph.db");
    syntaxmesh_store_turso::TursoGraphStore::migrate(&database)?;
    let invoke = |command: &str, store: &std::path::Path, arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(command)
            .arg(store)
            .args(arguments)
            .output()
    };
    let index = |command: &str, store: &std::path::Path| -> Result<(), Box<dyn Error>> {
        let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(command)
            .arg(&root)
            .arg(store)
            .output()?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
        }
        Ok(())
    };
    let read = |command: &str,
                store: &std::path::Path,
                generation: &str,
                relation: &str|
     -> Result<Value, Box<dyn Error>> {
        let output = invoke(command, store, &[generation, relation])?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let mut baseline = None;
    for (index_command, cycles_command, store) in [
        ("index", "cycles", file.as_path()),
        ("index-turso", "cycles-turso", database.as_path()),
    ] {
        index(index_command, store)?;
        let old = read(cycles_command, store, "current", "calls")?;
        let components = old
            .get("components")
            .and_then(Value::as_array)
            .ok_or("components missing")?;
        if components.len() != 1
            || components.first().and_then(Value::as_array).map(Vec::len) != Some(2)
        {
            return Err("mutually calling source did not produce one two-node cycle".into());
        }
        if baseline
            .as_ref()
            .is_some_and(|expected| expected != components)
        {
            return Err("File/Turso cycle components differ".into());
        }
        baseline = Some(components.clone());
        if read(cycles_command, store, "current", "imports")?.get("components")
            != Some(&serde_json::json!([]))
        {
            return Err("relation filter mixed call cycles into imports".into());
        }
        let generation = old
            .get("generation")
            .and_then(Value::as_str)
            .ok_or("generation missing")?;
        std::fs::write(&source, "pub fn alpha() { beta(); }\npub fn beta() {}\n")?;
        index(index_command, store)?;
        if read(cycles_command, store, "current", "calls")?.get("components")
            != Some(&serde_json::json!([]))
            || read(cycles_command, store, generation, "calls")? != old
        {
            return Err("cycle retraction changed retained history".into());
        }
        for arguments in [["bad", "calls"], ["current", "bad"]] {
            let rejected = invoke(cycles_command, store, &arguments)?;
            if rejected.status.success() || !rejected.stdout.is_empty() {
                return Err("invalid cycle arguments accepted".into());
            }
        }
        std::fs::write(
            &source,
            "pub fn alpha() { beta(); }\npub fn beta() { alpha(); }\n",
        )?;
    }
    Ok(())
}
