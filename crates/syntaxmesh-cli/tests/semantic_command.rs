//! A Rust subprocess fixture with a plain stdin/JSON-stdout harness contract.

use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use serde_json::{Value, json};
use syntaxmesh_core::NodeKind;

#[path = "support/semantic_store.rs"]
mod semantic_store;
use semantic_store::StoreHost;

#[path = "support/semantic_eval_config.rs"]
mod semantic_eval_config;
#[path = "support/semantic_evaluation.rs"]
mod semantic_evaluation;
#[path = "support/semantic_review.rs"]
mod semantic_review;

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "--fixture") {
        return fixture(&args);
    }
    if args.get(1).is_some_and(|arg| arg == "--evaluation") {
        semantic_evaluation::run(semantic_eval_config::EvaluationConfig::from_environment()?)?;
        return Ok(());
    }
    for host in [StoreHost::File, StoreHost::TursoVerified] {
        scenario(host)?;
    }
    for cross_document in [false, true] {
        evaluation_scenario(cross_document)?;
    }
    println!(
        "semantic command: File/verified-Turso cache, edit, failure, retained history and shared evaluator passed"
    );
    Ok(())
}

fn evaluation_scenario(cross_document: bool) -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let counter = directory.path().join("evaluation.counter");
    let executable = std::env::current_exe()?;
    let argv = serde_json::to_string(&json!([
        executable,
        "--fixture",
        counter,
        "success",
        "{model}"
    ]))?;
    let path = directory.path().join("command.json");
    fs::write(&path, argv)?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .current_dir(directory.path())
        .arg("--evaluation")
        .env("SYNTAXMESH_SEMANTIC_EVAL_COMMAND", "@command.json")
        .env(
            "SYNTAXMESH_SEMANTIC_EVAL_WORKING_DIRECTORY",
            directory.path(),
        )
        .env("SYNTAXMESH_SEMANTIC_EVAL_MODEL", "fixture-model")
        .env("SYNTAXMESH_SEMANTIC_EVAL_REVISION", "fixture-v1")
        .env(
            "SYNTAXMESH_SEMANTIC_EVAL_CROSS_DOCUMENT",
            cross_document.to_string(),
        )
        .env("SYNTAXMESH_SEMANTIC_EVAL_ALLOW_NETWORK", "false")
        .env_remove("SYNTAXMESH_SEMANTIC_EVAL_ENDPOINT")
        .output()?;
    semantic_evaluation::require_success(&output)?;
    let stdout = String::from_utf8(output.stdout)?;
    let evidence_root = stdout
        .lines()
        .find_map(|line| line.strip_prefix("live semantic evidence: "))
        .ok_or("evaluation evidence missing")?;
    let report: Value =
        serde_json::from_slice(&fs::read(Path::new(evidence_root).join("report.json"))?)?;
    for field in [
        "cache_stable",
        "edit_cache_selective",
        "edited_cache_stable",
    ] {
        if report.get(field).and_then(Value::as_bool) != Some(true) {
            return Err("command evaluation cache/history invariant failed".into());
        }
    }
    if report.get("transport").and_then(Value::as_str) != Some("local-command")
        || report.get("model").and_then(Value::as_str) != Some("fixture-model")
        || report.get("schema_version").and_then(Value::as_u64) != Some(5)
        || fs::read_to_string(counter)? != if cross_document { "4" } else { "2" }
        || !report
            .get("usage_scope")
            .and_then(Value::as_str)
            .is_some_and(|scope| scope.starts_with("unavailable"))
    {
        return Err(
            "command evaluation did not retain correct transport/model/call evidence".into(),
        );
    }
    Ok(())
}

fn fixture(args: &[String]) -> Result<(), Box<dyn Error>> {
    let count_path = args.get(2).ok_or("fixture counter missing")?;
    let count = fs::read_to_string(count_path)
        .unwrap_or_default()
        .parse::<usize>()
        .unwrap_or(0);
    fs::write(count_path, count.saturating_add(1).to_string())?;
    let mode = args.get(3).ok_or("fixture mode missing")?;
    let selected_model = args.get(4).ok_or("fixture model missing")?;
    if !matches!(selected_model.as_str(), "fixture-model" | "alternate-model") {
        return Err("selected model did not reach harness argv".into());
    }
    if Path::new("README.md").exists() {
        return Err("harness inherited indexed root".into());
    }
    match mode.as_str() {
        "invalid" => {
            println!("not JSON");
            return Ok(());
        }
        "failure" => {
            std::process::exit(7);
        }
        "oversized" => {
            std::io::stdout().write_all(&vec![b'x'; 4 * 1024 * 1024 + 1])?;
            return Ok(());
        }
        "oversized-diagnostics" => {
            std::io::stderr().write_all(&vec![b'x'; 4 * 1024 * 1024 + 1])?;
            return Ok(());
        }
        "invalid-utf8" => {
            std::io::stdout().write_all(&[0xff])?;
            return Ok(());
        }
        "success" => {}
        _ => return Err("unknown fixture mode".into()),
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let payload = input
        .split_once("\n\nInput:\n")
        .ok_or("missing shared prompt")?
        .1;
    let request: Value = serde_json::from_str(payload)?;
    let mut claims = Vec::new();
    for group in request
        .get("requests")
        .and_then(Value::as_array)
        .ok_or("missing requests")?
    {
        for chunk in group
            .get("chunks")
            .and_then(Value::as_array)
            .ok_or("missing chunks")?
        {
            let quote = chunk
                .get("text")
                .and_then(Value::as_str)
                .ok_or("missing source text")?;
            let hash = chunk
                .get("content_hash")
                .and_then(Value::as_str)
                .ok_or("missing content hash")?;
            claims.push(
                json!({"subject":"SyntaxMesh", "relation":"states", "object":quote,
                "evidence":[{"chunk_content_hash":hash,"quote":quote}]}),
            );
        }
    }
    println!("{}", json!({"claims":claims}));
    Ok(())
}

fn scenario(host: StoreHost) -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    host.prepare(root)?;
    fs::write(
        root.join("README.md"),
        "# Workflows\n\nSyntaxMesh uses Penelope.\n",
    )?;
    fs::write(
        root.join("history.md"),
        "# History\n\nSyntaxMesh keeps history.\n",
    )?;
    let counter = root.join("calls.counter");
    let executable = std::env::current_exe()?;
    let command = |mode: &str| {
        serde_json::to_string(&json!([executable, "--fixture", counter, mode, "{model}"]))
    };
    let argv = command("success")?;
    let argv_path = root.join("harness.json");
    fs::write(&argv_path, &argv)?;
    let file_argument = format!("@{}", argv_path.display());
    let options = [
        "--semantic",
        "fixture-model",
        "--semantic-command",
        file_argument.as_str(),
        "--semantic-model-revision=fixture-v1",
    ];
    let initial = host.run(root, None, &options)?;
    if !initial.status.success() {
        return Err(format!(
            "command indexing failed: {}",
            String::from_utf8_lossy(&initial.stderr)
        )
        .into());
    }
    let store = host.open(root)?;
    let generation = host.latest(store.as_ref())?.generation;
    let original = store.historical_snapshot(generation)?;
    let claims = original.nodes.iter().filter(|node| matches!(&node.kind, NodeKind::External {namespace,kind} if namespace == "syntaxmesh.semantic" && kind == "claim")).count();
    if claims != 2 || fs::read_to_string(&counter)? != "1" {
        return Err("command did not pack/cache both documents".into());
    }
    drop(store);
    if !host.run(root, None, &options)?.status.success() || fs::read_to_string(&counter)? != "1" {
        return Err("warm command launched inference".into());
    }
    fs::write(
        root.join("history.md"),
        "# History\n\nSyntaxMesh retains accepted history.\n",
    )?;
    if !host.run(root, None, &options)?.status.success() || fs::read_to_string(&counter)? != "2" {
        return Err("sparse edit did not reuse document cache".into());
    }
    let changed_store = host.open(root)?;
    let changed_generation = host.latest(changed_store.as_ref())?.generation;
    let changed = changed_store.historical_snapshot(changed_generation)?;
    if changed_generation == generation
        || changed_store.historical_snapshot(generation)? != original
    {
        return Err("command edit damaged history".into());
    }
    drop(changed_store);
    let offline = [
        "--semantic",
        "fixture-model",
        "--semantic-command",
        argv.as_str(),
        "--semantic-model-revision=fixture-v1",
        "--semantic-offline",
    ];
    if !host.run(root, None, &offline)?.status.success() || fs::read_to_string(&counter)? != "2" {
        return Err("offline command reuse launched inference".into());
    }
    let alternate = [
        "--semantic",
        "alternate-model",
        "--semantic-command",
        file_argument.as_str(),
        "--semantic-model-revision=fixture-v1",
    ];
    if !host.run(root, None, &alternate)?.status.success() || fs::read_to_string(&counter)? != "3" {
        return Err("changed model reused another model's inference cache".into());
    }
    if !host.run(root, None, &options)?.status.success() || fs::read_to_string(&counter)? != "3" {
        return Err("returning to original model did not reuse its cache".into());
    }
    let selected_store = host.open(root)?;
    let selected_generation = host.latest(selected_store.as_ref())?.generation;
    let selected = selected_store.historical_snapshot(selected_generation)?;
    if selected_store.historical_snapshot(changed_generation)? != changed
        || selected_store.historical_snapshot(generation)? != original
    {
        return Err("model switching damaged retained history".into());
    }
    drop(selected_store);
    for mode in [
        "invalid",
        "failure",
        "oversized",
        "oversized-diagnostics",
        "invalid-utf8",
    ] {
        let bad_argv = command(mode)?;
        let bad_options = [
            "--semantic",
            "fixture-model",
            "--semantic-command",
            bad_argv.as_str(),
            "--semantic-model-revision=fixture-v1",
        ];
        if host.run(root, None, &bad_options)?.status.success() {
            return Err(format!("accepted {mode} harness output").into());
        }
        let failed_store = host.open(root)?;
        if host.latest(failed_store.as_ref())?.generation != selected_generation
            || failed_store.historical_snapshot(selected_generation)? != selected
            || failed_store.historical_snapshot(changed_generation)? != changed
            || failed_store.historical_snapshot(generation)? != original
        {
            return Err("failed command replaced accepted history".into());
        }
    }
    Ok(())
}
