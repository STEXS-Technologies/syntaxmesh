use std::error::Error;
use std::fs;
use syntaxmesh_core::RelationKind;

#[path = "support/semantic_store.rs"]
mod store_host;
use store_host::StoreHost;

#[test]
fn file_config_only_inputs_survive_process_restart() -> Result<(), Box<dyn Error>> {
    lifecycle(StoreHost::File, false)
}

#[test]
fn verified_turso_config_only_inputs_survive_process_restart() -> Result<(), Box<dyn Error>> {
    lifecycle(StoreHost::TursoVerified, false)
}

#[test]
fn file_package_imports_only_edit_survives_process_restart() -> Result<(), Box<dyn Error>> {
    lifecycle(StoreHost::File, true)
}

#[test]
fn verified_turso_package_imports_only_edit_survives_process_restart() -> Result<(), Box<dyn Error>>
{
    lifecycle(StoreHost::TursoVerified, true)
}

fn lifecycle(host: StoreHost, package: bool) -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    host.prepare(root)?;
    fs::write(
        root.join("syntaxmesh.toml"),
        "[module_resolution]\nprofile = \"node\"\n",
    )?;
    let (input_path, before_config, after_config, caller) = if package {
        (
            "package.json",
            r##"{"type":"module","imports":{"#chosen":"./before.ts"}}"##,
            r##"{"type":"module","imports":{"#chosen":"./after.ts"}}"##,
            "import { chosen } from '#chosen'; chosen();",
        )
    } else {
        fs::write(root.join("tsconfig.json"), r#"{"extends":"./base.json"}"#)?;
        (
            "base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@chosen":["./before.ts"]}}}"#,
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@chosen":["./after.ts"]}}}"#,
            "import { chosen } from '@chosen'; chosen();",
        )
    };
    fs::write(root.join(input_path), before_config)?;
    fs::write(root.join("caller.ts"), caller)?;
    fs::write(root.join("before.ts"), "export function chosen() {}")?;
    fs::write(root.join("after.ts"), "export function chosen() {}")?;
    run(host, root)?;
    let store = host.open(root)?;
    let first = host.latest(store.as_ref())?.generation;
    let historical = store.historical_snapshot(first)?;
    drop(store);
    run(host, root)?;
    let repeated = host.open(root)?;
    if host.latest(repeated.as_ref())?.generation != first {
        return Err("unchanged process restart republished".into());
    }
    drop(repeated);
    fs::write(root.join(input_path), after_config)?;
    run(host, root)?;
    let updated = host.open(root)?;
    let current = host.latest(updated.as_ref())?.generation;
    let snapshot = updated.historical_snapshot(current)?;
    let old_edges = historical
        .edges
        .iter()
        .filter(|edge| edge.relation == RelationKind::ResolvesTo)
        .collect::<Vec<_>>();
    let new_edges = snapshot
        .edges
        .iter()
        .filter(|edge| edge.relation == RelationKind::ResolvesTo)
        .collect::<Vec<_>>();
    if current == first
        || old_edges.is_empty()
        || old_edges == new_edges
        || updated.historical_snapshot(first)? != historical
    {
        return Err("config-only process transition lost resolution/history".into());
    }
    drop(updated);
    run(host, root)?;
    let final_store = host.open(root)?;
    if host.latest(final_store.as_ref())?.generation != current {
        return Err("config-only process transition was not idempotent".into());
    }
    Ok(())
}

fn run(host: StoreHost, root: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let output = host.run(root, None, &[])?;
    if !output.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(())
}
