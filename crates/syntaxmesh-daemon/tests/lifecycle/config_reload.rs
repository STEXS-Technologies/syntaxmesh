use super::*;
use syntaxmesh_store::GraphStore;

#[test]
fn config_input_creation_and_deletion_refresh_running_owner() -> Result<(), Box<dyn Error>> {
    for polling in [false, true] {
        for package in [false, true] {
            let fixture = tempfile::tempdir()?;
            let root = fixture.path().join("source");
            std::fs::create_dir(&root)?;
            std::fs::write(root.join("lib.rs"), "fn policy_source() {}")?;
            std::fs::write(root.join("target.ts"), "export function target() {}")?;
            std::fs::write(
                root.join("syntaxmesh.toml"),
                "[module_resolution]\nprofile = \"node\"\n",
            )?;
            let (input, configuration, caller) = if package {
                (
                    "package.json",
                    r##"{"type":"module","imports":{"#chosen":"./target.ts"}}"##,
                    "import { target as policy_alias } from '#chosen'; policy_alias();",
                )
            } else {
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"baseUrl":".","paths":{"@chosen":["./target.ts"]}}}"#,
                    "import { target as policy_alias } from '@chosen'; policy_alias();",
                )
            };
            std::fs::write(root.join("caller.ts"), caller)?;
            let database = fixture.path().join("graph.db");
            TursoGraphStore::migrate(&database)?;
            let (mut process, base) = start(&root, &database, polling)?;
            let client = Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()?;
            let first = generation(&search(&client, &base, "policy_source", None)?)?.to_owned();
            require_import_resolution(&client, &base, &first, false)?;
            let path = root.join(input);
            std::fs::write(&path, configuration)?;
            let configured = changed_generation(&client, &base, &first)?;
            require_import_resolution(&client, &base, &configured, true)?;
            require_import_resolution(&client, &base, &first, false)?;
            std::fs::remove_file(&path)?;
            let removed = changed_generation(&client, &base, &configured)?;
            require_import_resolution(&client, &base, &removed, false)?;
            require_import_resolution(&client, &base, &configured, true)?;
            process.0.kill()?;
            process.0.wait()?;
            audit_history(&root, &database)?;
        }
    }
    Ok(())
}

#[test]
fn source_target_changes_refresh_long_lived_resolver_without_policy_reload()
-> Result<(), Box<dyn Error>> {
    for polling in [false, true] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("lib.rs"), "fn policy_source() {}")?;
        std::fs::write(
            root.join("caller.ts"),
            "import { target as policy_alias } from './target'; policy_alias();",
        )?;
        std::fs::write(
            root.join("syntaxmesh.toml"),
            "[module_resolution]\nprofile = \"node\"\n",
        )?;
        let database = fixture.path().join("graph.db");
        TursoGraphStore::migrate(&database)?;
        let (mut process, base) = start(&root, &database, polling)?;
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()?;
        let first = generation(&search(&client, &base, "policy_source", None)?)?.to_owned();
        require_import_resolution(&client, &base, &first, false)?;
        let target = root.join("target.ts");
        std::fs::write(&target, "export function target() {}")?;
        let added = changed_generation(&client, &base, &first)?;
        require_import_resolution(&client, &base, &added, true)?;
        require_import_resolution(&client, &base, &first, false)?;
        std::fs::remove_file(&target)?;
        let removed = changed_generation(&client, &base, &added)?;
        require_import_resolution(&client, &base, &removed, false)?;
        require_import_resolution(&client, &base, &added, true)?;
        std::fs::write(&target, "export function target() { return 1; }")?;
        let restored = changed_generation(&client, &base, &removed)?;
        require_import_resolution(&client, &base, &restored, true)?;
        require_import_resolution(&client, &base, &removed, false)?;
        process.0.kill()?;
        process.0.wait()?;
        audit_history(&root, &database)?;
    }
    Ok(())
}

#[test]
fn verification_reload_without_override_preserves_gap_and_fails_closed()
-> Result<(), Box<dyn Error>> {
    for polling in [false, true] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        std::fs::create_dir(&root)?;
        let config = root.join("syntaxmesh.toml");
        let source = root.join("lib.rs");
        std::fs::write(&config, "[history]\nverified = true\n")?;
        std::fs::write(&source, "fn policy_source() {}")?;
        let database = fixture.path().join("graph.db");
        TursoGraphStore::migrate(&database)?;
        // This scenario requires failure, not successful timed shutdown. The
        // bounded wait_status check and Process drop guard still bound cleanup.
        let (mut process, base) =
            start_with_policy_duration(&root, &database, polling, false, false, None)?;
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()?;
        let first = generation(&search(&client, &base, "policy_source", None)?)?.to_owned();
        require_status(&client, &base, "Verified")?;
        std::fs::write(&config, "[history]\nverified = false\n")?;
        std::fs::write(&source, "fn policy_source() { let value = 1; }")?;
        let durable = changed_generation(&client, &base, &first)?;
        require_status(&client, &base, "Durable")?;
        std::fs::write(&config, "[history]\nverified = true\n")?;
        std::fs::write(&source, "fn policy_source() { let value = 2; }")?;
        if let Err(error) = wait_status(&mut process, false) {
            let observed = client
                .get(format!("{base}/api/v1/status"))
                .send()
                .and_then(reqwest::blocking::Response::error_for_status)
                .and_then(reqwest::blocking::Response::text);
            return Err(format!(
                "verification reload failed (polling={polling}, first={first}, durable={durable}): {error}; observed status={observed:?}"
            )
            .into());
        }
        let _ownership = syntaxmesh_ownership_host::WriterLease::acquire(&database)?;
        let store = TursoGraphStore::open(&database)?;
        let history = store.generation_history()?;
        let latest = history.last().ok_or("missing verification transition")?;
        let manifest = store.manifest(latest.manifest.generation)?;
        if history.len() != 3
            || manifest.status != syntaxmesh_core::GenerationStatus::VerificationFailed
        {
            return Err(
                "verification gap was silently accepted or missing publication was hidden".into(),
            );
        }
        let original = history
            .iter()
            .find(|entry| entry.manifest.generation.0.to_hex() == first)
            .ok_or("missing verified generation")?;
        let intermediate = history
            .iter()
            .find(|entry| entry.manifest.generation.0.to_hex() == durable)
            .ok_or("missing durable generation")?;
        if original.manifest.generation.0.to_hex() != first
            || intermediate.manifest.generation.0.to_hex() != durable
            || store.manifest(original.manifest.generation)?.status
                != syntaxmesh_core::GenerationStatus::Durable
            || store.manifest(intermediate.manifest.generation)?.status
                != syntaxmesh_core::GenerationStatus::Durable
            || manifest.parent != Some(intermediate.manifest.generation)
        {
            return Err(format!("verification transition status/lineage differs: original={:?}, intermediate={:?}, latest={manifest:?}", store.manifest(original.manifest.generation)?, store.manifest(intermediate.manifest.generation)?).into());
        }
        drop(store);
        drop(_ownership);
        audit_history(&root, &database)?;
    }
    Ok(())
}

fn require_status(client: &Client, base: &str, expected: &str) -> Result<(), Box<dyn Error>> {
    let response: Value = client
        .get(format!("{base}/api/v1/status"))
        .send()?
        .error_for_status()?
        .json()?;
    if response
        .pointer("/data/manifest/status")
        .and_then(Value::as_str)
        != Some(expected)
    {
        return Err(format!("unexpected verification status: {response}").into());
    }
    Ok(())
}

#[test]
fn project_resolver_reload_and_deletion_publish_without_restarting_owner()
-> Result<(), Box<dyn Error>> {
    for polling in [false, true] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("lib.rs"), "fn policy_source() {}")?;
        std::fs::write(
            root.join("caller.ts"),
            "import { target as policy_alias } from './target'; policy_alias();",
        )?;
        std::fs::write(root.join("target.ts"), "export function target() {}")?;
        let database = fixture.path().join("graph.db");
        TursoGraphStore::migrate(&database)?;
        let (mut process, base) = start(&root, &database, polling)?;
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()?;
        let first = generation(&search(&client, &base, "policy_source", None)?)?.to_owned();
        require_import_resolution(&client, &base, &first, false)?;
        std::fs::write(
            root.join("syntaxmesh.toml"),
            "[history]\nverified = false\n[module_resolution]\nprofile = \"node\"\n",
        )?;
        let configured = changed_generation(&client, &base, &first)?;
        require_import_resolution(&client, &base, &configured, true)?;
        std::fs::remove_file(root.join("syntaxmesh.toml"))?;
        let restored = changed_generation(&client, &base, &configured)?;
        require_import_resolution(&client, &base, &restored, false)?;
        require_import_resolution(&client, &base, &configured, true)?;
        if restored == first || !has_nodes(&search(&client, &base, "policy_source", Some(&first))?)
        {
            return Err("policy reload lost transition identity or retained evidence".into());
        }
        std::fs::write(root.join("syntaxmesh.toml"), "[invalid")?;
        wait_status(&mut process, false)?;
        if syntaxmesh_ownership_host::is_writer_active(&database)? {
            return Err("invalid config retained daemon ownership".into());
        }
        std::fs::remove_file(root.join("syntaxmesh.toml"))?;
        audit_history(&root, &database)?;
    }
    Ok(())
}

fn require_import_resolution(
    client: &Client,
    base: &str,
    selected: &str,
    expected: bool,
) -> Result<(), Box<dyn Error>> {
    let response = search(client, base, "policy_alias", Some(selected))?;
    let occurrence = response
        .get("data")
        .and_then(Value::as_array)
        .and_then(|nodes| {
            nodes.iter().find(|node| {
                node.get("kind")
                    .and_then(|kind| kind.get("Import"))
                    .is_some()
            })
        })
        .ok_or("missing import occurrence")?;
    let id: syntaxmesh_core::NodeId =
        serde_json::from_value(occurrence.get("id").ok_or("missing import ID")?.clone())?;
    let neighbors: Value = client
        .get(format!("{base}/api/v1/nodes/{}/neighbors", id.0.to_hex()))
        .query(&[("generation", selected)])
        .send()?
        .error_for_status()?
        .json()?;
    let resolved = neighbors
        .get("data")
        .and_then(|data| data.get("items"))
        .and_then(Value::as_array)
        .ok_or("missing import neighbor page")?
        .iter()
        .any(|item| {
            item.get("edge")
                .and_then(|edge| edge.get("relation"))
                .and_then(Value::as_str)
                == Some("ResolvesTo")
        });
    if resolved != expected {
        return Err(
            format!("resolver reload produced unexpected target edges: {neighbors}").into(),
        );
    }
    Ok(())
}

fn changed_generation(
    client: &Client,
    base: &str,
    previous: &str,
) -> Result<String, Box<dyn Error>> {
    let started = Instant::now();
    loop {
        let response = search(client, base, "policy_source", None)?;
        let current = generation(&response)?;
        if has_nodes(&response) && current != previous {
            return Ok(current.to_owned());
        }
        if started.elapsed() > Duration::from_secs(3) {
            return Err("configuration reload did not publish changed resolver policy".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
