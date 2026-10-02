//! Repeated, provenance-captured real-source context retrieval evaluation.

use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use chrono::Utc;
use serde_json::{Value, json};

type BenchResult<T> = Result<T, Box<dyn Error>>;

const EVALUATION_TEST: &str = "context_real_source_retrieval_evaluation";
const SIBLING_EVALUATION_TEST: &str = "context_sibling_source_retrieval_evaluation";

pub(super) fn run(workspace: &Path) -> BenchResult<()> {
    let arguments = env::args().skip(2).collect::<Vec<_>>();
    if arguments.len() > 1 {
        return Err("usage: cargo make benchmark-context-retrieval [OUTPUT_ROOT]".into());
    }
    let output_root = arguments.first().map_or_else(
        || workspace.join("target/context-retrieval-results"),
        PathBuf::from,
    );
    let output_root = absolute_path(&output_root)?;
    let iterations = env::var("SYNTAXMESH_CONTEXT_BENCH_ITERATIONS")
        .unwrap_or_else(|_| "3".to_owned())
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or("SYNTAXMESH_CONTEXT_BENCH_ITERATIONS must be a positive integer")?;
    let evaluation_root = env::var_os("SYNTAXMESH_CONTEXT_EVAL_ROOT")
        .map(PathBuf::from)
        .map(|path| -> BenchResult<PathBuf> {
            let path = if path.is_absolute() {
                path
            } else {
                workspace.join(path)
            };
            Ok(fs::canonicalize(path)?)
        })
        .transpose()?;
    if let Some(root) = &evaluation_root {
        let origin = git_value(root, &["remote", "get-url", "origin"])
            .ok_or("benchmark source needs a verifiable origin")?;
        if !matches!(
            origin.trim(),
            "git@github.com:STEXS-Technologies/shardline.git"
                | "git@workstation:STEXS-Technologies/shardline.git"
                | "https://github.com/STEXS-Technologies/shardline.git"
                | "https://github.com/STEXS-Technologies/shardline"
        ) {
            return Err(
                "external context benchmark currently supports only STEXS-Technologies/shardline"
                    .into(),
            );
        }
    }
    let evaluation_scope =
        env::var("SYNTAXMESH_CONTEXT_EVAL_SCOPE").unwrap_or_else(|_| "selected".to_owned());
    if !matches!(evaluation_scope.as_str(), "selected" | "repository")
        || (evaluation_scope == "repository" && evaluation_root.is_none())
    {
        return Err("SYNTAXMESH_CONTEXT_EVAL_SCOPE must be selected or repository; repository requires SYNTAXMESH_CONTEXT_EVAL_ROOT".into());
    }
    validate_indexed_host_scope(
        evaluation_root.is_some(),
        env::var_os("SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE").is_some(),
    )?;
    let role_preference =
        env::var("SYNTAXMESH_CONTEXT_ROLE_PREFERENCE").unwrap_or_else(|_| "neutral".to_owned());
    let syntax_policy =
        env::var("SYNTAXMESH_CONTEXT_SYNTAX_POLICY").unwrap_or_else(|_| "strict".to_owned());
    if !matches!(syntax_policy.as_str(), "strict" | "record-failures") {
        return Err("context syntax policy must be strict or record-failures".into());
    }
    let source_content = env::var_os("SYNTAXMESH_CONTEXT_SOURCE_CONTENT_PROBE").is_some();
    let discovery_preference = env::var("SYNTAXMESH_CONTEXT_DISCOVERY_PREFERENCE")
        .unwrap_or_else(|_| "balanced".to_owned());
    if !matches!(discovery_preference.as_str(), "balanced" | "lexical-first") {
        return Err("invalid discovery packing preference".into());
    }
    if discovery_preference == "lexical-first"
        && env::var_os("SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE").is_none()
    {
        return Err("lexical-first requires indexed host probe".into());
    }
    if source_content && env::var_os("SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE").is_none() {
        return Err("source-content probe requires indexed host probe".into());
    }
    validate_role_preference(
        &role_preference,
        env::var_os("SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE").is_some(),
    )?;
    fs::create_dir_all(&output_root)?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.6fZ").to_string();
    let revision = git_value(workspace, &["rev-parse", "--short=12", "HEAD"]);
    let dirty =
        git_value(workspace, &["status", "--porcelain"]).is_some_and(|status| !status.is_empty());
    let run_dir = output_root.join(format!(
        "context-{timestamp}-{}",
        revision.as_deref().unwrap_or("uncommitted")
    ));
    if run_dir.exists() {
        return Err(format!(
            "refusing to overwrite evaluation run: {}",
            run_dir.display()
        )
        .into());
    }
    let raw_dir = run_dir.join("raw");
    fs::create_dir_all(&raw_dir)?;

    let evaluation_test = if evaluation_root.is_some() {
        SIBLING_EVALUATION_TEST
    } else {
        EVALUATION_TEST
    };
    let build_profile =
        env::var("SYNTAXMESH_CONTEXT_BENCH_PROFILE").unwrap_or_else(|_| "debug".to_owned());
    if !matches!(build_profile.as_str(), "debug" | "release") {
        return Err("SYNTAXMESH_CONTEXT_BENCH_PROFILE must be debug or release".into());
    }
    let mut command = vec!["cargo", "test"];
    let index_build_metrics = env::var_os("SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE").is_some();
    if index_build_metrics {
        command.extend(["--features", "benchmark-instrumentation"]);
    }
    if build_profile == "release" {
        command.push("--release");
    }
    command.extend([
        "--locked",
        "-p",
        "syntaxmesh-mcp",
        "--lib",
        evaluation_test,
        "--",
        "--ignored",
        "--nocapture",
    ]);
    let rustc = Command::new("rustc").arg("--version").output().ok();
    let fixture_revision = evaluation_root
        .as_ref()
        .and_then(|root| git_value(root, &["rev-parse", "--short=12", "HEAD"]));
    let fixture_working_tree_dirty = evaluation_root.as_ref().and_then(|root| {
        git_value(root, &["status", "--porcelain", "--untracked-files=normal"])
            .map(|status| !status.is_empty())
    });
    let mut metadata = json!({
        "schema_version": 1,
        "revision": revision,
        "source_fingerprint_blake3": source_fingerprint(workspace)?,
        "working_tree_dirty": dirty,
        "timestamp_utc": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        "repository_root": workspace.display().to_string(),
        "fixture": evaluation_root.as_ref().map_or_else(
            || "crates/syntaxmesh-engine/src (RustExtractor)".to_owned(),
            |path| path.display().to_string(),
        ),
        "fixture_revision": fixture_revision,
        "fixture_working_tree_dirty": fixture_working_tree_dirty,
        "fixture_input_fingerprint_blake3": Value::Null,
        "evaluation_scope": evaluation_scope,
        "source_role_preference": role_preference,
        "source_syntax_policy": syntax_policy,
        "source_content_policy": source_content,
        "discovery_packing_preference": discovery_preference,
        "rust_min_stack": env::var("RUST_MIN_STACK").ok(),
        "diagnostic_probes": diagnostic_probes(|name| env::var_os(name).is_some()),
        "build_profile": build_profile,
        "index_build_metrics": index_build_metrics,
        "node_read_profile": index_build_metrics,
        "scratch_root": env::temp_dir().display().to_string(),
        "iterations": iterations,
        "budgets": [2048, 8192],
        "tokenizer": "tiktoken-rs-0.12.1/o200k_base/ordinary",
        "platform": format!("{}-{}", env::consts::OS, env::consts::ARCH),
        "cpu_model": cpu_model(),
        "logical_cpus": std::thread::available_parallelism().ok().map(usize::from),
        "rustc": rustc.as_ref().and_then(successful_stdout),
        "command": command,
    });
    write_json(&run_dir.join("metadata.json"), &metadata)?;

    let mut prebuild_command = Command::new("cargo");
    prebuild_command.current_dir(workspace).args([
        "test",
        "--locked",
        "-p",
        "syntaxmesh-mcp",
        "--lib",
        "--no-run",
    ]);
    if build_profile == "release" {
        prebuild_command.arg("--release");
    }
    if index_build_metrics {
        prebuild_command.args(["--features", "benchmark-instrumentation"]);
    }
    let prebuild = logged_output(&mut prebuild_command, &raw_dir.join("prebuild"))?;
    if !prebuild.status.success() {
        return exit_error(&prebuild, &run_dir, "context evaluation prebuild failed");
    }

    let mut samples = Vec::with_capacity(iterations);
    let mut fixture_input_fingerprint = None;
    for iteration in 1..=iterations {
        let mut evaluation = Command::new("cargo");
        evaluation
            .current_dir(workspace)
            .args(command.iter().skip(1));
        if let Some(root) = &evaluation_root {
            evaluation.env("SYNTAXMESH_CONTEXT_EVAL_ROOT", root);
        }
        if index_build_metrics {
            evaluation.env("SYNTAXMESH_TURSO_NODE_READ_PROFILE", "1");
        }
        let output = logged_output(
            &mut evaluation,
            &raw_dir.join(format!("iteration-{iteration}")),
        )?;
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let results = parse_metrics(&combined)?;
        let fixture_inputs = parse_fixture_inputs(&combined)?;
        if !output.status.success() {
            samples.push(json!({"iteration": iteration, "success": false, "fixture_inputs": fixture_inputs, "results": results}));
            write_json(&run_dir.join("samples.json"), &json!(samples))?;
            return exit_error(
                &output,
                &run_dir,
                "context retrieval execution failed; inspect raw stderr",
            );
        }
        if evaluation_root.is_some() && fixture_inputs.is_none() {
            return Err(format!(
                "iteration {iteration} omitted fixture provenance; raw output retained at {}",
                run_dir.display()
            )
            .into());
        }
        if let Some(inputs) = &fixture_inputs {
            let fingerprint = inputs["input_fingerprint_blake3"]
                .as_str()
                .ok_or("fixture fingerprint is missing")?;
            if let Some(previous) = &fixture_input_fingerprint {
                if previous != fingerprint {
                    return Err(format!(
                        "selected fixture files changed between iterations: {previous} != {fingerprint}"
                    )
                    .into());
                }
            } else {
                fixture_input_fingerprint = Some(fingerprint.to_owned());
            }
        }
        let expected_cases = evaluation_root.as_ref().map_or(3, |root| {
            match root
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("shardline") => 7,
                _ => 5,
            }
        });
        validate_case_matrix(&results, expected_cases)?;
        write_json(
            &raw_dir.join(format!("iteration-{iteration}.json")),
            &json!(results),
        )?;
        samples.push(json!({
            "iteration": iteration,
            "success": output.status.success(),
            "fixture_inputs": fixture_inputs,
            "results": results,
        }));
        println!("completed context evaluation sample {iteration}/{iterations}");
    }

    if let Some(fingerprint) = fixture_input_fingerprint {
        metadata
            .as_object_mut()
            .ok_or("benchmark metadata is not an object")?
            .insert(
                "fixture_input_fingerprint_blake3".to_owned(),
                Value::String(fingerprint),
            );
    }
    write_json(&run_dir.join("metadata.json"), &metadata)?;
    write_json(&run_dir.join("samples.json"), &json!(samples))?;
    let summary = render_summary(&metadata, &samples);
    fs::write(run_dir.join("summary.md"), summary)?;
    println!("context evaluation evidence: {}", run_dir.display());
    Ok(())
}

fn parse_metrics(output: &str) -> BenchResult<Vec<Value>> {
    let mut rows = Vec::new();
    for line in output.lines() {
        let Some((_, metrics)) = line.split_once("context_eval ") else {
            continue;
        };
        let mut row = serde_json::Map::new();
        for metric in metrics.split_whitespace() {
            let Some((key, value)) = metric.split_once('=') else {
                continue;
            };
            let parsed = match key {
                "budget" | "tokens" | "items" | "omitted" | "query_us" => {
                    Value::from(value.parse::<u64>()?)
                }
                "retrieved" => Value::from(value.parse::<bool>()?),
                _ => Value::from(value),
            };
            row.insert(key.to_owned(), parsed);
        }
        for required in [
            "case",
            "target",
            "budget",
            "retrieved",
            "tokens",
            "items",
            "omitted",
            "query_us",
        ] {
            if !row.contains_key(required) {
                return Err(format!("context evaluation row lacks {required}: {line}").into());
            }
        }
        rows.push(Value::Object(row));
    }
    Ok(rows)
}

fn parse_fixture_inputs(output: &str) -> BenchResult<Option<Value>> {
    let mut fixture = None;
    for line in output.lines() {
        let Some((_, fields)) = line.split_once("context_fixture ") else {
            continue;
        };
        let mut parsed = serde_json::Map::new();
        for field in fields.split_whitespace() {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let value = if key == "files" {
                Value::from(value.parse::<u64>()?)
            } else {
                Value::from(value)
            };
            parsed.insert(key.to_owned(), value);
        }
        for key in ["corpus", "files", "input_fingerprint_blake3"] {
            if !parsed.contains_key(key) {
                return Err(format!("fixture provenance line lacks {key}: {line}").into());
            }
        }
        if fixture.replace(Value::Object(parsed)).is_some() {
            return Err("evaluation emitted multiple fixture provenance lines".into());
        }
    }
    Ok(fixture)
}

fn validate_case_matrix(rows: &[Value], expected_cases: usize) -> BenchResult<()> {
    let mut cases =
        std::collections::BTreeMap::<&str, (&str, std::collections::BTreeSet<u64>)>::new();
    for row in rows {
        let case = row
            .get("case")
            .and_then(Value::as_str)
            .ok_or("missing case")?;
        let target = row
            .get("target")
            .and_then(Value::as_str)
            .ok_or("missing target")?;
        let budget = row
            .get("budget")
            .and_then(Value::as_u64)
            .ok_or("missing budget")?;
        let (previous_target, budgets) = cases
            .entry(case)
            .or_insert_with(|| (target, Default::default()));
        if *previous_target != target || !budgets.insert(budget) {
            return Err(format!("duplicate case/budget or changed target: {case}").into());
        }
    }
    let expected = [2048_u64, 8192_u64]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    if cases.len() != expected_cases || cases.values().any(|(_, budgets)| *budgets != expected) {
        return Err(format!(
            "incomplete context matrix: expected {expected_cases} cases at both budgets"
        )
        .into());
    }
    Ok(())
}

fn render_summary(metadata: &Value, samples: &[Value]) -> String {
    let mut output = vec![
        "# Context retrieval evaluation".to_owned(),
        String::new(),
        format!("- measured at: `{}`", metadata["timestamp_utc"].as_str().unwrap_or("unknown")),
        format!("- source revision: `{}`", metadata["revision"].as_str().unwrap_or("unknown")),
        format!("- source fingerprint: `{}`", metadata["source_fingerprint_blake3"].as_str().unwrap_or("unknown")),
        format!("- working tree dirty: `{}`", metadata["working_tree_dirty"]),
        format!("- fixture: `{}`", metadata["fixture"].as_str().unwrap_or("unknown")),
        format!("- fixture revision: `{}`; dirty: `{}`", metadata["fixture_revision"].as_str().unwrap_or("unknown"), metadata["fixture_working_tree_dirty"]),
        format!("- selected fixture input BLAKE3: `{}`", metadata["fixture_input_fingerprint_blake3"].as_str().unwrap_or("unknown")),
        format!("- host: `{}`; CPU: `{}`", metadata["platform"].as_str().unwrap_or("unknown"), metadata["cpu_model"].as_str().unwrap_or("unknown")),
        format!("- toolchain: `{}`", metadata["rustc"].as_str().unwrap_or("unknown")),
        format!("- tokenizer: `{}`", metadata["tokenizer"].as_str().unwrap_or("unknown")),
        format!("- samples: {} fresh index/database runs", metadata["iterations"].as_u64().unwrap_or_default()),
        String::new(),
        "| token budget | target retrieval | median query latency | packed token range | omitted candidates |".to_owned(),
        "| ---: | ---: | ---: | ---: | ---: |".to_owned(),
    ];

    for budget in [2048_u64, 8192_u64] {
        let rows = samples
            .iter()
            .flat_map(|sample| sample["results"].as_array().into_iter().flatten())
            .filter(|row| row["budget"].as_u64() == Some(budget))
            .collect::<Vec<_>>();
        let hits = rows
            .iter()
            .filter(|row| row["retrieved"].as_bool() == Some(true))
            .count();
        let median_query_us = median(
            rows.iter()
                .filter_map(|row| row["query_us"].as_u64())
                .collect(),
        );
        let min_tokens = rows
            .iter()
            .filter_map(|row| row["tokens"].as_u64())
            .min()
            .unwrap_or_default();
        let max_tokens = rows
            .iter()
            .filter_map(|row| row["tokens"].as_u64())
            .max()
            .unwrap_or_default();
        let omitted = rows
            .iter()
            .filter_map(|row| row["omitted"].as_u64())
            .collect::<Vec<_>>();
        output.push(format!(
            "| {budget} | {hits}/{} | {} µs | {min_tokens}–{max_tokens} | {}–{} |",
            rows.len(),
            median_query_us.map_or_else(|| "n/a".to_owned(), |value| value.to_string()),
            omitted.iter().min().copied().unwrap_or_default(),
            omitted.iter().max().copied().unwrap_or_default(),
        ));
    }
    output.extend([
        String::new(),
        "Each sample uses a fresh temporary Turso database. Query latency includes source-backed context compilation and exact token counting, but excludes fixture indexing. These fixed-target retrieval cases are regression signals, not full-repository relevance or cross-machine performance claims.".to_owned(),
        "Raw stdout/stderr and per-iteration JSON are retained beside this summary.".to_owned(),
    ]);
    output.join("\n")
}

fn median(mut values: Vec<u64>) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}

fn write_json(path: &Path, value: &Value) -> BenchResult<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn logged_output(command: &mut Command, prefix: &Path) -> BenchResult<Output> {
    let stdout_path = prefix.with_extension("stdout.txt");
    let stderr_path = prefix.with_extension("stderr.txt");
    let stdout = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stdout_path)?;
    let stderr = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr_path)?;
    let status = command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .status()?;
    Ok(Output {
        status,
        stdout: fs::read(stdout_path)?,
        stderr: fs::read(stderr_path)?,
    })
}

fn exit_error<T>(output: &Output, run_dir: &Path, message: &str) -> BenchResult<T> {
    Err(format!(
        "{message} (exit {:?}); evidence retained at {}",
        output.status.code(),
        run_dir.display()
    )
    .into())
}

fn git_value(workspace: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(workspace)
        .args(arguments)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn successful_stdout(output: &Output) -> Option<String> {
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn cpu_model() -> Option<String> {
    fs::read_to_string("/proc/cpuinfo")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("model name\t: ").map(str::to_owned))
}

fn source_fingerprint(workspace: &Path) -> BenchResult<String> {
    let mut files = vec![
        "Cargo.toml",
        "Cargo.lock",
        "Makefile.toml",
        "rust-toolchain.toml",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect::<Vec<_>>();
    for directory in ["crates", "xtask", ".cargo"] {
        if workspace.join(directory).exists() {
            fingerprint_files(workspace, Path::new(directory), &mut files)?;
        }
    }
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for relative_path in files {
        let name = relative_path.to_string_lossy().replace('\\', "/");
        let contents = fs::read(workspace.join(relative_path))?;
        hasher.update(&(name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        hasher.update(&(contents.len() as u64).to_le_bytes());
        hasher.update(&contents);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn validate_role_preference(preference: &str, indexed: bool) -> Result<(), &'static str> {
    if !matches!(preference, "neutral" | "test-first" | "test-last") {
        return Err("SYNTAXMESH_CONTEXT_ROLE_PREFERENCE must be neutral, test-first or test-last");
    }
    if preference != "neutral" && !indexed {
        return Err("non-neutral role preference requires SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE");
    }
    Ok(())
}

const fn validate_indexed_host_scope(
    external_root: bool,
    indexed_host: bool,
) -> Result<(), &'static str> {
    if indexed_host && !external_root {
        return Err(
            "SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE requires SYNTAXMESH_CONTEXT_EVAL_ROOT; the engine-only fixture does not exercise indexed host routing",
        );
    }
    Ok(())
}

fn diagnostic_probes(enabled: impl Fn(&str) -> bool) -> Value {
    let names = [
        "SYNTAXMESH_CONTEXT_CORPUS_PROBE",
        "SYNTAXMESH_CONTEXT_IDENTIFIER_INDEX_PROBE",
        "SYNTAXMESH_CONTEXT_PATH_IDENTIFIER_INDEX_PROBE",
        "SYNTAXMESH_CONTEXT_GRAPH_SEED_PROBE",
        "SYNTAXMESH_CONTEXT_MIXED_SEED_PROBE",
        "SYNTAXMESH_CONTEXT_FAMILY_SEED_PROBE",
        "SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE",
        "SYNTAXMESH_CONTEXT_DEFINITION_SEED_PROBE",
        "SYNTAXMESH_CONTEXT_ALL_FACT_CHANNEL_SEED_PROBE",
        "SYNTAXMESH_CONTEXT_TARGET_SEED_PROBE",
    ];
    Value::Object(
        names
            .into_iter()
            .map(|name| (name.to_owned(), Value::Bool(enabled(name))))
            .collect(),
    )
}

fn fingerprint_files(
    workspace: &Path,
    relative: &Path,
    files: &mut Vec<PathBuf>,
) -> BenchResult<()> {
    for entry in fs::read_dir(workspace.join(relative))? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = relative.join(entry.file_name());
        if kind.is_symlink() {
            return Err(format!(
                "benchmark source fingerprint rejects symlink: {}",
                path.display()
            )
            .into());
        }
        if kind.is_dir() {
            if entry.file_name() != "target" && entry.file_name() != ".git" {
                fingerprint_files(workspace, &path, files)?;
            }
        } else if kind.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn absolute_path(path: &Path) -> BenchResult<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        env::current_dir()?.join(path)
    };
    Ok(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn role_policy_rejects_invalid_and_unrouted_preferences() {
        for indexed in [false, true] {
            assert!(super::validate_role_preference("neutral", indexed).is_ok());
            for invalid in ["", "production", "TestFirst", "test-first "] {
                assert!(super::validate_role_preference(invalid, indexed).is_err());
            }
        }
        for preference in ["test-first", "test-last"] {
            assert!(super::validate_role_preference(preference, true).is_ok());
            assert!(super::validate_role_preference(preference, false).is_err());
        }
    }

    #[test]
    fn case_matrix_rejects_partial_duplicate_and_changed_target_rows() {
        let rows = vec![
            serde_json::json!({"case":"one", "target":"symbol", "budget":2048}),
            serde_json::json!({"case":"one", "target":"symbol", "budget":8192}),
        ];
        assert!(super::validate_case_matrix(&rows, 1).is_ok());
        assert!(super::validate_case_matrix(&rows, 2).is_err());
        assert!(
            rows.get(..1)
                .is_some_and(|partial| super::validate_case_matrix(partial, 1).is_err())
        );
        let mut duplicate = rows.clone();
        duplicate.extend(rows);
        assert!(super::validate_case_matrix(&duplicate, 1).is_err());
        let changed = vec![
            serde_json::json!({"case":"one", "target":"symbol", "budget":2048}),
            serde_json::json!({"case":"one", "target":"other", "budget":8192}),
        ];
        assert!(super::validate_case_matrix(&changed, 1).is_err());
    }
    use super::{
        diagnostic_probes, logged_output, parse_fixture_inputs, parse_metrics, source_fingerprint,
    };

    #[test]
    fn indexed_host_policy_requires_an_external_fixture() {
        for (external_root, indexed_host) in [(false, false), (true, false), (true, true)] {
            assert!(super::validate_indexed_host_scope(external_root, indexed_host).is_ok());
        }
        assert!(matches!(
            super::validate_indexed_host_scope(false, true),
            Err(error) if error.contains("SYNTAXMESH_CONTEXT_EVAL_ROOT")
        ));
    }

    #[test]
    fn probe_metadata_records_enabled_and_disabled_policies() -> Result<(), String> {
        let metadata =
            diagnostic_probes(|name| name == "SYNTAXMESH_CONTEXT_PATH_IDENTIFIER_INDEX_PROBE");
        let object = metadata
            .as_object()
            .ok_or("probe metadata is not an object")?;
        if object.len() != 10
            || object.get("SYNTAXMESH_CONTEXT_PATH_IDENTIFIER_INDEX_PROBE")
                != Some(&serde_json::Value::Bool(true))
            || object
                .iter()
                .filter(|(_, value)| **value == serde_json::Value::Bool(false))
                .count()
                != 9
        {
            return Err(format!("probe metadata lost policy settings: {metadata}"));
        }
        let indexed = diagnostic_probes(|name| name == "SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE");
        if indexed.get("SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE")
            != Some(&serde_json::Value::Bool(true))
        {
            return Err("indexed host policy was not captured".to_owned());
        }
        let targeted = diagnostic_probes(|name| name == "SYNTAXMESH_CONTEXT_TARGET_SEED_PROBE");
        if targeted.get("SYNTAXMESH_CONTEXT_TARGET_SEED_PROBE")
            != Some(&serde_json::Value::Bool(true))
        {
            return Err("target seed policy was not captured".to_owned());
        }
        Ok(())
    }

    #[test]
    fn source_fingerprint_covers_nested_inputs_and_ignores_build_output()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path();
        for name in [
            "Cargo.toml",
            "Cargo.lock",
            "Makefile.toml",
            "rust-toolchain.toml",
        ] {
            std::fs::write(root.join(name), name)?;
        }
        std::fs::create_dir_all(root.join("crates/example/src"))?;
        std::fs::create_dir_all(root.join("crates/example/target"))?;
        let source = root.join("crates/example/src/extractor.rs");
        std::fs::write(&source, "before")?;
        let before = source_fingerprint(root)?;
        std::fs::write(root.join("crates/example/target/artifact"), "ignored")?;
        if source_fingerprint(root)? != before {
            return Err(std::io::Error::other("build output changed source fingerprint").into());
        }
        std::fs::write(&source, "after")?;
        let after = source_fingerprint(root)?;
        std::fs::rename(&source, source.with_file_name("renamed.rs"))?;
        if after == before || source_fingerprint(root)? == after {
            return Err(std::io::Error::other("source edit or rename escaped fingerprint").into());
        }
        Ok(())
    }

    #[test]
    fn child_output_is_file_backed_and_existing_evidence_is_not_overwritten()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let prefix = temporary.path().join("sample");
        let output = logged_output(
            std::process::Command::new("rustc").arg("--version"),
            &prefix,
        )?;
        if !output.status.success()
            || output.stdout.is_empty()
            || std::fs::read(prefix.with_extension("stdout.txt"))? != output.stdout
            || std::fs::read(prefix.with_extension("stderr.txt"))? != output.stderr
            || logged_output(
                std::process::Command::new("rustc").arg("--version"),
                &prefix,
            )
            .is_ok()
            || std::fs::read(prefix.with_extension("stdout.txt"))? != output.stdout
        {
            return Err(std::io::Error::other(
                "file-backed child output lost or overwrote evidence",
            )
            .into());
        }
        let failed = logged_output(
            std::process::Command::new("rustc").arg("--syntaxmesh-invalid-option"),
            &temporary.path().join("failed"),
        )?;
        if failed.status.success() || failed.stderr.is_empty() {
            return Err(
                std::io::Error::other("failed child omitted its diagnostic evidence").into(),
            );
        }
        Ok(())
    }

    #[test]
    fn parses_evaluation_metrics_without_losing_integer_precision() -> Result<(), String> {
        let rows = parse_metrics(
            "context_eval case=lineage target=::publish_lineage budget=2048 retrieved=true tokens=2037 items=14 omitted=117 query_us=842286\n",
        )
        .map_err(|error| error.to_string())?;
        let Some(row) = rows.first() else {
            return Err("context evaluation metrics were not parsed".to_owned());
        };
        if rows.len() != 1
            || row.get("retrieved").and_then(serde_json::Value::as_bool) != Some(true)
            || row.get("query_us").and_then(serde_json::Value::as_u64) != Some(842_286)
            || row.get("target").and_then(serde_json::Value::as_str) != Some("::publish_lineage")
        {
            return Err(format!("parsed evaluation row incorrectly: {rows:?}"));
        }
        Ok(())
    }

    #[test]
    fn parses_selected_fixture_provenance() -> Result<(), String> {
        let fixture = parse_fixture_inputs(
            "context_fixture corpus=syntaxmesh files=4 input_fingerprint_blake3=abc123\n",
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "fixture provenance was not parsed".to_owned())?;
        if fixture.get("corpus").and_then(serde_json::Value::as_str) != Some("syntaxmesh")
            || fixture.get("files").and_then(serde_json::Value::as_u64) != Some(4)
            || fixture
                .get("input_fingerprint_blake3")
                .and_then(serde_json::Value::as_str)
                != Some("abc123")
        {
            return Err(format!("unexpected fixture provenance: {fixture}"));
        }
        Ok(())
    }
}
