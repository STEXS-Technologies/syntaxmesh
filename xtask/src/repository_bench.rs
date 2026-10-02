//! Repeated, isolated repository benchmark evidence collection.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Map, Number, Value, json};

type BenchResult<T> = Result<T, Box<dyn Error>>;
const BACKENDS: [&str; 3] = ["in-memory", "sqlite", "turso"];
const PHASES: [&str; 2] = ["initial", "incremental"];
const STORE_DELTA_STAGES: [&str; 13] = [
    "lineage_request_validation",
    "lineage_prior_state_lookup",
    "delta_preflight",
    "mutation_plan",
    "candidate_state_and_reference_validation",
    "persistent_root_update",
    "current_state_and_indexes",
    "generation_history_append",
    "lineage_graph_delta_apply",
    "lineage_delta_validation",
    "lineage_candidate_ready",
    "consequence_validation",
    "consequence_candidate_ready",
];
const IN_MEMORY_STORE_DELTA_STAGES: [&str; 15] = [
    "lineage_candidate_clone",
    "lineage_request_validation",
    "lineage_prior_state_lookup",
    "delta_preflight",
    "mutation_plan",
    "candidate_state_and_reference_validation",
    "persistent_root_update",
    "current_state_and_indexes",
    "generation_history_append",
    "lineage_graph_delta_apply",
    "lineage_delta_validation",
    "lineage_candidate_ready",
    "consequence_validation",
    "consequence_candidate_ready",
    "lineage_candidate_publish",
];
const PENELOPE_STAGES: [&str; 4] = [
    "publish_recovery",
    "record_read_prepare_cas",
    "graph_commit",
    "completion_record_cas",
];
const TURSO_STAGES: [&str; 21] = [
    "validate_history_and_build_delta_mutations",
    "temporal_fact_versions",
    "persistent_fact_root",
    "persistent_incidence_tree_apply",
    "persistent_incidence_page_persistence",
    "persistent_incidence_root_row",
    "persistent_incidence_root",
    "current_projection_file_rows",
    "current_projection_provenance_rows",
    "current_projection_edge_removals",
    "current_projection_node_removals",
    "current_projection_node_encoding",
    "current_projection_node_statement_execution",
    "current_projection_node_upserts",
    "current_projection_edge_encoding",
    "current_projection_edge_statement_execution",
    "current_projection_edge_upserts",
    "delta_reference_validation",
    "history_lineage_and_consequence_projection",
    "acceptance_and_checkpoint",
    "transaction_commit",
];
const SQLITE_STAGES: [&str; 22] = [
    "incident_edge_cascade",
    "begin_validate_and_build_delta_mutations",
    "temporal_fact_encode",
    "temporal_sql_write",
    "temporal_sql_close",
    "temporal_fact_versions",
    "persistent_tree_mutation_work",
    "persistent_tree_page_load",
    "persistent_tree_reachable_dirty",
    "persistent_tree_page_encode",
    "persistent_tree_page_insert",
    "persistent_incidence_apply",
    "persistent_incidence_page_load",
    "persistent_incidence_reachable_dirty",
    "persistent_incidence_page_encode",
    "persistent_incidence_page_insert",
    "persistent_incidence_root_record_write",
    "persistent_fact_root",
    "manifest_and_current_projection",
    "history_lineage_consequence_and_acceptance",
    "checkpoint_writes",
    "transaction_commit",
];
const SQLITE_WRAPPER_STAGES: [&str; 4] = [
    "candidate_store_clone",
    "candidate_graph_delta",
    "persist_transaction",
    "publish_candidate_to_handle",
];

struct Patterns {
    scan: regex::Regex,
    backend: regex::Regex,
    profile: regex::Regex,
    storage: regex::Regex,
    resource: regex::Regex,
    penelope: regex::Regex,
    turso: regex::Regex,
    sqlite: regex::Regex,
    store_delta: regex::Regex,
}

impl Patterns {
    fn new() -> BenchResult<Self> {
        Ok(Self {
            scan: regex::Regex::new(
                r"repository=(?P<root>.+) rust_files=(?P<files>\d+) source_bytes=(?P<bytes>\d+) scan_elapsed_us=(?P<elapsed>\d+)",
            )?,
            backend: regex::Regex::new(
                r"backend=(?P<backend>[\w-]+) initial_index_ms=(?P<initial>\d+) incremental_one_file_ms=(?P<incremental>\d+) files=(?P<files>\d+) nodes=(?P<nodes>\d+) edges=(?P<edges>\d+) provenance=(?P<provenance>\d+)",
            )?,
            profile: regex::Regex::new(
                r"profile backend=(?P<backend>[\w-]+) phase=(?P<phase>[\w-]+) recovery_ms=(?P<recovery>\d+) prepare_delta_ms=(?P<prepare>\d+) penelope_publish_ms=(?P<publish>\d+) total_index_ms=(?P<total>\d+)",
            )?,
            storage: regex::Regex::new(
                r"backend=(?P<backend>[\w-]+) persisted_database_bytes=(?P<bytes>\d+)",
            )?,
            resource: regex::Regex::new(
                r"resource backend=(?P<backend>[\w-]+) stage=(?P<stage>[\w-]+) rss_kib=(?P<rss>\d+) peak_rss_kib=(?P<peak>\d+) peak_delta_kib=(?P<delta>\d+)",
            )?,
            penelope: regex::Regex::new(
                r"penelope_stage stage=(?P<stage>[\w-]+) elapsed_us=(?P<elapsed>\d+)",
            )?,
            turso: regex::Regex::new(
                r"turso_stage stage=(?P<stage>[\w-]+) elapsed_us=(?P<elapsed>\d+)",
            )?,
            sqlite: regex::Regex::new(
                r"sqlite_stage stage=(?P<stage>[\w-]+) elapsed_us=(?P<elapsed>\d+)",
            )?,
            store_delta: regex::Regex::new(
                r"store_delta_stage stage=(?P<stage>[\w-]+) elapsed_us=(?P<elapsed>\d+)",
            )?,
        })
    }
}

pub(super) fn run(workspace: &Path) -> BenchResult<()> {
    let arguments: Vec<String> = env::args().skip(2).collect();
    if arguments.len() > 1 {
        return Err("usage: cargo make benchmark-repository-evidence [OUTPUT_ROOT]".into());
    }
    let output_root = arguments
        .first()
        .map_or_else(|| workspace.join("target/benchmark-results"), PathBuf::from);
    let output_root = absolute_path(&output_root)?;
    let repository = env::var_os("SYNTAXMESH_BENCH_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.to_owned());
    let repository = absolute_path(&repository)?;
    let selected_backends = selected_backends()?;
    let iterations = env::var("SYNTAXMESH_BENCH_ITERATIONS")
        .unwrap_or_else(|_| "3".into())
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or("SYNTAXMESH_BENCH_ITERATIONS must be a positive integer")?;
    fs::create_dir_all(&output_root)?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.6fZ").to_string();
    let revision = git_value(workspace, &["rev-parse", "--short=12", "HEAD"]);
    let dirty =
        git_value(workspace, &["status", "--porcelain"]).is_some_and(|value| !value.is_empty());
    let run_dir = output_root.join(format!(
        "repository-{timestamp}-{}",
        revision.as_deref().unwrap_or("uncommitted")
    ));
    if run_dir.exists() {
        return Err(format!("refusing to overwrite benchmark run: {}", run_dir.display()).into());
    }
    let results_dir = run_dir.join("results");
    fs::create_dir_all(&results_dir)?;

    let patterns = Patterns::new()?;
    let gnu_time = gnu_time_binary();
    let rustc = command_output("rustc", &["--version"]);
    let metadata = json!({
        "schema_version": 6,
        "revision": revision,
        "working_tree_dirty": dirty,
        "timestamp_utc": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        "repository_root": repository.display().to_string(),
        "iterations": iterations,
        "command": ["cargo", "make", "benchmark-repository"],
        "platform": format!("{}-{}", env::consts::OS, env::consts::ARCH),
        "machine": env::consts::ARCH,
        "cpu_model": cpu_model(),
        "logical_cpus": std::thread::available_parallelism().ok().map(usize::from),
        "rustc": successful_stdout(rustc.as_ref()),
        "peak_rss_measurement": gnu_time.as_ref().map(|_| "GNU time process-tree maximum RSS in KiB"),
        "backend_isolation": true,
        "backends": selected_backends,
    });
    write_json(&run_dir.join("metadata.json"), &metadata)?;

    let prebuild = Command::new("cargo")
        .current_dir(workspace)
        .args([
            "bench",
            "-p",
            "syntaxmesh-store-turso",
            "--bench",
            "repository_indexing",
            "--features",
            "benchmark-instrumentation",
            "--locked",
            "--no-run",
        ])
        .output()?;
    write_output(&results_dir.join("prebuild"), &prebuild)?;
    if !prebuild.status.success() {
        eprintln!(
            "benchmark prebuild failed; evidence retained at {}",
            run_dir.display()
        );
        return exit_error(prebuild.status, "repository benchmark prebuild failed");
    }

    let mut samples = Vec::new();
    for iteration in 1..=iterations {
        let mut backend_samples = Map::new();
        for backend in &selected_backends {
            let database_dir = run_dir
                .join("storage")
                .join(format!("iteration-{iteration}"))
                .join(backend);
            fs::create_dir_all(&database_dir)?;
            let prefix = results_dir.join(format!("iteration-{iteration}-{backend}"));
            let time_path = prefix.with_extension("time.txt");
            let environment = benchmark_environment(&database_dir, &repository, backend);
            let mut command = vec![
                "cargo".to_owned(),
                "make".to_owned(),
                "benchmark-repository".to_owned(),
            ];
            if let Some(time_binary) = &gnu_time {
                command = vec![
                    time_binary.display().to_string(),
                    "-f".into(),
                    "%M\\t%x".into(),
                    "-o".into(),
                    time_path.display().to_string(),
                    "cargo".into(),
                    "make".into(),
                    "benchmark-repository".into(),
                ];
            }
            let started = Instant::now();
            let output = run_child(&command, workspace, &environment)?;
            let elapsed_seconds = started.elapsed().as_secs_f64();
            write_output(&prefix, &output)?;
            if !output.status.success() {
                eprintln!(
                    "benchmark failed for {backend}, iteration {iteration}; evidence retained at {}",
                    run_dir.display()
                );
                return exit_error(output.status, "repository benchmark process failed");
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let parsed = match parse_output(&stdout, &stderr, backend, &patterns) {
                Ok(parsed) => parsed,
                Err(error) => {
                    eprintln!("{error}; evidence retained at {}", run_dir.display());
                    return Err(error);
                }
            };
            let peak_rss = match (&gnu_time, read_time_measurement(&time_path, output.status)) {
                (Some(_), Ok(value)) => Some(value),
                (Some(_), Err(error)) => {
                    eprintln!("could not parse GNU time resource evidence: {error}");
                    return Err(error);
                }
                (None, _) => None,
            };
            let metrics = parsed
                .get("backends")
                .and_then(|all| all.get(backend))
                .cloned()
                .ok_or("parsed benchmark omitted selected backend")?;
            let breakdown = match backend.as_str() {
                "sqlite" => storage_breakdown(&database_dir.join("repository.db")),
                "turso" => storage_breakdown(&database_dir.join("repository-turso.db")),
                _ => json!({"available":false,"reason":"in-memory backend"}),
            };
            let backend_sample = json!({
                "elapsed_seconds": elapsed_seconds,
                "peak_rss_kib": peak_rss,
                "resource_stages": parsed.get("resource_stages").cloned().unwrap_or_else(|| json!([])),
                "publication_stages": parsed.get("publication_stages").cloned().unwrap_or_else(|| json!({})),
                "metrics": metrics,
                "database_breakdown": breakdown,
            });
            write_json(&prefix.with_extension("json"), &backend_sample)?;
            backend_samples.insert(backend.clone(), backend_sample);
            fs::remove_dir_all(&database_dir)?;
            println!(
                "iteration={iteration}/{iterations} backend={backend} elapsed_seconds={elapsed_seconds:.3}"
            );
        }
        let sample = json!({"iteration":iteration,"backends":backend_samples});
        samples.push(sample.clone());
        write_json(
            &results_dir.join(format!("iteration-{iteration}.json")),
            &sample,
        )?;
    }
    write_json(&run_dir.join("samples.json"), &json!(samples))?;
    fs::write(
        run_dir.join("summary.md"),
        render_summary(&metadata, &samples)?,
    )?;
    println!("benchmark_evidence={}", run_dir.display());
    Ok(())
}

fn selected_backends() -> BenchResult<Vec<String>> {
    let Some(value) = env::var_os("SYNTAXMESH_BENCH_BACKENDS") else {
        return Ok(BACKENDS
            .iter()
            .map(|backend| (*backend).to_owned())
            .collect());
    };
    let requested: Vec<String> = value
        .to_string_lossy()
        .split(',')
        .map(str::trim)
        .filter(|backend| !backend.is_empty())
        .map(str::to_owned)
        .collect();
    let unique: BTreeSet<&str> = requested.iter().map(String::as_str).collect();
    if requested.is_empty()
        || unique.len() != requested.len()
        || requested
            .iter()
            .any(|backend| !BACKENDS.contains(&backend.as_str()))
    {
        return Err(format!(
            "SYNTAXMESH_BENCH_BACKENDS must be a unique comma-separated subset of {}",
            BACKENDS.join(", ")
        )
        .into());
    }
    Ok(BACKENDS
        .iter()
        .filter(|backend| unique.contains(**backend))
        .map(|backend| (*backend).to_owned())
        .collect())
}

fn benchmark_environment(
    database_dir: &Path,
    repository: &Path,
    backend: &str,
) -> BTreeMap<std::ffi::OsString, std::ffi::OsString> {
    let mut environment: BTreeMap<_, _> = env::vars_os().collect();
    for (key, value) in [
        ("SYNTAXMESH_BENCH_DB_DIR", database_dir.as_os_str()),
        ("SYNTAXMESH_BENCH_ROOT", repository.as_os_str()),
        ("SYNTAXMESH_BENCH_KEEP", std::ffi::OsStr::new("1")),
        ("SYNTAXMESH_BENCH_BACKEND", std::ffi::OsStr::new(backend)),
        ("SYNTAXMESH_BENCH_MEMORY_STAGES", std::ffi::OsStr::new("1")),
        ("SYNTAXMESH_PENELOPE_PROFILE", std::ffi::OsStr::new("1")),
        ("SYNTAXMESH_SQLITE_PROFILE", std::ffi::OsStr::new("1")),
        ("SYNTAXMESH_TURSO_PROFILE", std::ffi::OsStr::new("1")),
        ("SYNTAXMESH_STORE_PROFILE", std::ffi::OsStr::new("1")),
    ] {
        environment.insert(key.into(), value.to_owned());
    }
    environment
}

fn parse_output(
    stdout: &str,
    stderr: &str,
    selected: &str,
    patterns: &Patterns,
) -> BenchResult<Value> {
    let mut backends = BTreeMap::<String, Map<String, Value>>::new();
    let mut result = Map::new();
    let mut resources = Vec::new();
    for line in stdout.lines() {
        if let Some(capture) = patterns.scan.captures(line) {
            result.insert(
                "scan".into(),
                json!({
                    "repository": capture_value(&capture, "root"),
                    "rust_files": capture_u64(&capture, "files")?,
                    "source_bytes": capture_u64(&capture, "bytes")?,
                    "scan_elapsed_us": capture_u64(&capture, "elapsed")?,
                }),
            );
        }
        if let Some(capture) = patterns.backend.captures(line) {
            let backend = capture_value(&capture, "backend");
            backends.entry(backend).or_default().extend([
                (
                    "initial_index_ms".into(),
                    json!(capture_u64(&capture, "initial")?),
                ),
                (
                    "incremental_one_file_ms".into(),
                    json!(capture_u64(&capture, "incremental")?),
                ),
                ("files".into(), json!(capture_u64(&capture, "files")?)),
                ("nodes".into(), json!(capture_u64(&capture, "nodes")?)),
                ("edges".into(), json!(capture_u64(&capture, "edges")?)),
                (
                    "provenance".into(),
                    json!(capture_u64(&capture, "provenance")?),
                ),
            ]);
        }
        if let Some(capture) = patterns.profile.captures(line) {
            let backend = capture_value(&capture, "backend");
            let phase = capture_value(&capture, "phase");
            let phase_value = json!({
                "recovery_ms": capture_u64(&capture, "recovery")?,
                "prepare_delta_ms": capture_u64(&capture, "prepare")?,
                "penelope_publish_ms": capture_u64(&capture, "publish")?,
                "total_index_ms": capture_u64(&capture, "total")?,
            });
            let entry = backends.entry(backend).or_default();
            let profiles = entry.entry("profile_stages").or_insert_with(|| json!({}));
            if let Some(object) = profiles.as_object_mut() {
                object.insert(phase, phase_value);
            }
        }
        if let Some(capture) = patterns.storage.captures(line) {
            backends
                .entry(capture_value(&capture, "backend"))
                .or_default()
                .insert(
                    "persisted_database_bytes".into(),
                    json!(capture_u64(&capture, "bytes")?),
                );
        }
        if let Some(capture) = patterns.resource.captures(line) {
            resources.push(json!({
                "backend": capture_value(&capture, "backend"),
                "stage": capture_value(&capture, "stage"),
                "rss_kib": capture_u64(&capture, "rss")?,
                "peak_rss_kib": capture_u64(&capture, "peak")?,
                "peak_delta_kib": capture_u64(&capture, "delta")?,
            }));
        }
    }
    if !result.contains_key("scan") || !backends.contains_key(selected) {
        return Err(format!("benchmark output is missing scan or {selected} metrics").into());
    }
    if backends.len() != 1 || !backends.contains_key(selected) {
        return Err(
            format!("isolated {selected} run unexpectedly included another backend").into(),
        );
    }
    let phases = backends
        .get(selected)
        .and_then(|backend| backend.get("profile_stages"))
        .and_then(Value::as_object)
        .ok_or_else(|| format!("benchmark output omitted profile stages for {selected}"))?;
    if phases.len() != PHASES.len() || PHASES.iter().any(|phase| !phases.contains_key(*phase)) {
        return Err(format!("benchmark output is missing profile stages for {selected}").into());
    }
    let publication = parse_publication(stderr, selected, patterns)?;
    result.insert("backends".into(), json!(backends));
    result.insert("resource_stages".into(), json!(resources));
    result.insert("publication_stages".into(), publication);
    Ok(Value::Object(result))
}

fn parse_publication(stderr: &str, selected: &str, patterns: &Patterns) -> BenchResult<Value> {
    let mut samples: BTreeMap<&str, Vec<(String, u64)>> = BTreeMap::new();
    for line in stderr.lines() {
        for (name, pattern) in [
            ("penelope", &patterns.penelope),
            ("turso", &patterns.turso),
            ("sqlite", &patterns.sqlite),
            ("store_delta", &patterns.store_delta),
        ] {
            if let Some(capture) = pattern.captures(line) {
                samples.entry(name).or_default().push((
                    capture_value(&capture, "stage"),
                    capture_u64(&capture, "elapsed")?,
                ));
            }
        }
    }
    let expected_penelope: Vec<&str> = PHASES.iter().flat_map(|_| PENELOPE_STAGES).collect();
    verify_sequence("Penelope", &samples, "penelope", &expected_penelope)?;
    let store_delta_stages: &[&str] = match selected {
        "in-memory" => &IN_MEMORY_STORE_DELTA_STAGES,
        "sqlite" => &STORE_DELTA_STAGES,
        "turso" => &[],
        _ => return Err(format!("unsupported selected backend: {selected}").into()),
    };
    if store_delta_stages.is_empty() {
        if samples.contains_key("store_delta") {
            return Err("unexpected shared graph-store diagnostics in Turso benchmark".into());
        }
    } else {
        let expected_store_delta: Vec<&str> = PHASES
            .iter()
            .flat_map(|_| store_delta_stages.iter().copied())
            .collect();
        verify_sequence(
            "shared graph-store delta",
            &samples,
            "store_delta",
            &expected_store_delta,
        )?;
    }
    let mut publication = json!({"initial":{},"incremental":{}});
    for (index, phase) in PHASES.iter().enumerate() {
        for (stage_index, stage) in store_delta_stages.iter().enumerate() {
            let sample_index = index
                .checked_mul(STORE_DELTA_STAGES.len())
                .and_then(|offset| offset.checked_add(stage_index))
                .ok_or("shared graph-store stage index overflow")?;
            let elapsed = samples
                .get("store_delta")
                .and_then(|items| items.get(sample_index))
                .map(|(_, elapsed)| *elapsed)
                .ok_or("missing shared graph-store stage")?;
            insert_json(
                &mut publication,
                phase,
                &format!("store_delta.{stage}"),
                milliseconds_number(elapsed)?,
            )?;
        }
        for (stage_index, stage) in PENELOPE_STAGES.iter().enumerate() {
            let sample_index = index
                .checked_mul(PENELOPE_STAGES.len())
                .and_then(|offset| offset.checked_add(stage_index))
                .ok_or("Penelope stage index overflow")?;
            let elapsed = samples
                .get("penelope")
                .and_then(|items| items.get(sample_index))
                .map(|(_, elapsed)| *elapsed)
                .ok_or("missing Penelope stage")?;
            insert_json(
                &mut publication,
                phase,
                &format!("penelope.{stage}"),
                milliseconds_number(elapsed)?,
            )?;
        }
    }
    match selected {
        "turso" => {
            let expected: Vec<&str> = PHASES.iter().flat_map(|_| TURSO_STAGES).collect();
            verify_sequence("Turso", &samples, "turso", &expected)?;
            for (index, phase) in PHASES.iter().enumerate() {
                for (stage_index, stage) in TURSO_STAGES.iter().enumerate() {
                    let sample_index = index
                        .checked_mul(TURSO_STAGES.len())
                        .and_then(|offset| offset.checked_add(stage_index))
                        .ok_or("Turso stage index overflow")?;
                    let elapsed = samples
                        .get("turso")
                        .and_then(|items| items.get(sample_index))
                        .map(|(_, elapsed)| *elapsed)
                        .ok_or("missing Turso stage")?;
                    insert_json(
                        &mut publication,
                        phase,
                        &format!("turso.{stage}"),
                        milliseconds_number(elapsed)?,
                    )?;
                }
            }
            if samples.contains_key("sqlite") {
                return Err("unexpected SQLite diagnostics in Turso benchmark".into());
            }
        }
        "sqlite" => {
            let stages: Vec<&str> = [SQLITE_WRAPPER_STAGES[0], SQLITE_WRAPPER_STAGES[1]]
                .into_iter()
                .chain(SQLITE_STAGES)
                .chain([SQLITE_WRAPPER_STAGES[2], SQLITE_WRAPPER_STAGES[3]])
                .collect();
            let expected: Vec<&str> = PHASES.iter().flat_map(|_| stages.iter().copied()).collect();
            verify_sequence("SQLite", &samples, "sqlite", &expected)?;
            for (index, phase) in PHASES.iter().enumerate() {
                for (stage_index, stage) in stages.iter().enumerate() {
                    let sample_index = index
                        .checked_mul(stages.len())
                        .and_then(|offset| offset.checked_add(stage_index))
                        .ok_or("SQLite stage index overflow")?;
                    let elapsed = samples
                        .get("sqlite")
                        .and_then(|items| items.get(sample_index))
                        .map(|(_, elapsed)| *elapsed)
                        .ok_or("missing SQLite stage")?;
                    insert_json(
                        &mut publication,
                        phase,
                        &format!("sqlite.{stage}"),
                        milliseconds_number(elapsed)?,
                    )?;
                }
            }
            if samples.contains_key("turso") {
                return Err("unexpected Turso diagnostics in SQLite benchmark".into());
            }
        }
        "in-memory" if samples.contains_key("sqlite") || samples.contains_key("turso") => {
            return Err("unexpected database diagnostics in in-memory benchmark".into());
        }
        "in-memory" => {}
        _ => return Err(format!("unsupported selected backend: {selected}").into()),
    }
    Ok(publication)
}

fn verify_sequence(
    label: &str,
    samples: &BTreeMap<&str, Vec<(String, u64)>>,
    name: &str,
    expected: &[&str],
) -> BenchResult<()> {
    let observed: Vec<&str> = samples
        .get(name)
        .into_iter()
        .flatten()
        .map(|(stage, _)| stage.as_str())
        .collect();
    if observed != expected {
        return Err(format!(
            "benchmark diagnostics have unexpected {label} stage sequence: {observed:?}"
        )
        .into());
    }
    Ok(())
}

fn storage_breakdown(database: &Path) -> Value {
    if !database.is_file() {
        return json!({"available":false,"reason":"database file missing"});
    }
    let result = (|| -> rusqlite::Result<Value> {
        let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut statement = connection.prepare("SELECT name, COUNT(*), SUM(pgsize), SUM(payload) FROM dbstat GROUP BY name ORDER BY SUM(pgsize) DESC")?;
        let rows = statement
            .query_map([], |row| {
                Ok(json!({
                    "name": row.get::<_, String>(0)?,
                    "pages": row.get::<_, i64>(1)?,
                    "page_bytes": row.get::<_, i64>(2)?,
                    "payload_bytes": row.get::<_, i64>(3)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let page_count: i64 = connection.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let page_size: i64 = connection.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        Ok(json!({"available":true,"page_count":page_count,"page_size":page_size,"objects":rows}))
    })();
    result.unwrap_or_else(|error| json!({"available":false,"reason":error.to_string()}))
}

fn render_summary(metadata: &Value, samples: &[Value]) -> BenchResult<String> {
    let backends = metadata
        .get("backends")
        .and_then(Value::as_array)
        .ok_or("metadata omitted backend list")?;
    let mut rows = Vec::new();
    let mut profile_rows = Vec::new();
    let mut publication_rows = Vec::new();
    let mut resource_rows = Vec::new();
    let mut storage_rows = Vec::new();
    let mut storage_notes = Vec::new();
    for backend_value in backends {
        let backend = backend_value
            .as_str()
            .ok_or("backend name is not a string")?;
        let backend_samples = samples
            .iter()
            .map(|sample| {
                sample
                    .get("backends")
                    .and_then(|indexed_backends| indexed_backends.get(backend))
                    .ok_or_else(|| -> Box<dyn Error> { "sample omitted selected backend".into() })
            })
            .collect::<BenchResult<Vec<_>>>()?;
        let initial = values_at(&backend_samples, &["metrics", "initial_index_ms"])?;
        let incremental = values_at(&backend_samples, &["metrics", "incremental_one_file_ms"])?;
        let storage =
            values_optional_at(&backend_samples, &["metrics", "persisted_database_bytes"]);
        let peak = values_optional_at(&backend_samples, &["peak_rss_kib"]);
        rows.push(format!(
            "| {backend} | {} | {} | {} | {} |",
            comma(median(&initial).unwrap_or_default()),
            median(&incremental).map_or_else(|| "n/a".into(), comma),
            median(&storage).map_or_else(|| "n/a".into(), comma),
            median(&peak).map_or_else(|| "n/a".into(), |value| scaled(value, 1024, 1))
        ));
        for phase in PHASES {
            let recovery = values_at_profiles(&backend_samples, phase, "recovery_ms")?;
            let prepare = values_at_profiles(&backend_samples, phase, "prepare_delta_ms")?;
            let publish = values_at_profiles(&backend_samples, phase, "penelope_publish_ms")?;
            let total = values_at_profiles(&backend_samples, phase, "total_index_ms")?;
            profile_rows.push(format!(
                "| {backend} | {phase} | {} | {} | {} | {} |",
                median(&recovery).map_or_else(|| "n/a".into(), comma),
                median(&prepare).map_or_else(|| "n/a".into(), comma),
                median(&publish).map_or_else(|| "n/a".into(), comma),
                median(&total).map_or_else(|| "n/a".into(), comma),
            ));
            let first = backend_samples.first().ok_or("no benchmark samples")?;
            let stage_map = first
                .get("publication_stages")
                .and_then(|value| value.get(phase))
                .and_then(Value::as_object)
                .ok_or("sample omitted publication timings")?;
            for stage in stage_map.keys() {
                let elapsed = backend_samples
                    .iter()
                    .map(|sample| {
                        sample
                            .get("publication_stages")
                            .and_then(|value| value.get(phase))
                            .and_then(|value| value.get(stage))
                            .and_then(number_to_microseconds)
                            .ok_or_else(|| -> Box<dyn Error> {
                                "sample omitted publication stage timing".into()
                            })
                    })
                    .collect::<BenchResult<Vec<_>>>()?;
                publication_rows.push(format!(
                    "| {backend} | {phase} | `{stage}` | {} |",
                    median(&elapsed).map_or_else(|| "n/a".into(), format_milliseconds_microseconds)
                ));
            }
        }
        let mut resource_names = BTreeSet::new();
        for sample in &backend_samples {
            for resource in sample
                .get("resource_stages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if resource.get("backend").and_then(Value::as_str) == Some(backend)
                    && let Some(name) = resource.get("stage").and_then(Value::as_str)
                {
                    resource_names.insert(name.to_owned());
                }
            }
        }
        for stage in resource_names {
            let rss = resource_values(&backend_samples, backend, &stage, "rss_kib")?;
            let delta = resource_values(&backend_samples, backend, &stage, "peak_delta_kib")?;
            resource_rows.push(format!(
                "| {backend} | {stage} | {} | {} |",
                median(&rss).map_or_else(|| "n/a".into(), |value| scaled(value, 1024, 1)),
                median(&delta).map_or_else(|| "n/a".into(), |value| scaled(value, 1024, 1))
            ));
        }
        if ["sqlite", "turso"].contains(&backend) {
            let mut object_names = BTreeSet::new();
            for sample in &backend_samples {
                let breakdown = sample
                    .get("database_breakdown")
                    .ok_or("sample missing database breakdown")?;
                if breakdown.get("available").and_then(Value::as_bool) == Some(true) {
                    for item in breakdown
                        .get("objects")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(name) = item.get("name").and_then(Value::as_str) {
                            object_names.insert(name.to_owned());
                        }
                    }
                } else {
                    storage_notes.push(format!(
                        "{backend} dbstat unavailable: {}",
                        breakdown
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown reason")
                    ));
                }
            }
            let mut ranked: Vec<String> = object_names.into_iter().collect();
            ranked.sort_by_key(|name| {
                std::cmp::Reverse(
                    storage_values(&backend_samples, name, "page_bytes")
                        .into_iter()
                        .max()
                        .unwrap_or_default(),
                )
            });
            for name in ranked {
                let pages = storage_values(&backend_samples, &name, "page_bytes");
                let payloads = storage_values(&backend_samples, &name, "payload_bytes");
                storage_rows.push(format!(
                    "| {backend} | `{name}` | {} | {} |",
                    median(&pages).map_or_else(|| "n/a".into(), comma),
                    median(&payloads).map_or_else(|| "n/a".into(), comma)
                ));
            }
        }
    }
    let mut source = metadata
        .get("revision")
        .and_then(Value::as_str)
        .unwrap_or("uncommitted workspace")
        .to_owned();
    if metadata.get("working_tree_dirty").and_then(Value::as_bool) == Some(true) {
        source.push_str(" (dirty)");
    }
    let storage_notes = storage_notes
        .into_iter()
        .map(|note| format!("- {note}"))
        .collect::<Vec<_>>();
    let lines = [
        "# SyntaxMesh repository-indexing benchmark".to_owned(), String::new(),
        format!("- source: `{source}`"),
        format!("- measured at: `{}`", metadata.get("timestamp_utc").and_then(Value::as_str).unwrap_or("unknown")),
        format!("- fixture: `{}`", metadata.get("repository_root").and_then(Value::as_str).unwrap_or("unknown")),
        format!("- host: {} ({})", metadata.get("platform").and_then(Value::as_str).unwrap_or("unknown"), metadata.get("logical_cpus").and_then(Value::as_u64).map_or_else(|| "unknown CPUs".into(), |n| format!("{n} logical CPUs"))),
        format!("- toolchain: `{}`", metadata.get("rustc").and_then(Value::as_str).unwrap_or("unknown")),
        format!("- samples: {} fresh database runs", metadata.get("iterations").and_then(Value::as_u64).unwrap_or_default()),
        String::new(), "| backend | median initial ms | median one-file incremental ms | median retained DB bytes | median peak RSS MiB |".into(), "| --- | ---: | ---: | ---: | ---: |".into(),
    ].into_iter().chain(rows).chain([
        String::new(), "## Indexing stage medians (milliseconds)".into(), String::new(),
        "`prepare_delta` includes indexed-store reads, extraction, and reference resolution. `penelope_publish` includes the Penelope journal and atomic store publication.".into(), String::new(),
        "| backend | phase | pre-recovery | prepare delta | Penelope publish | total index |".into(), "| --- | --- | ---: | ---: | ---: | ---: |".into(),
    ]).chain(profile_rows).chain([
        String::new(), "## Publication substage medians (milliseconds)".into(), String::new(),
        "These are nested timing segments. Shared store and adapter rows are inside Penelope `graph_commit`; do not add them to the parent stage or to each other.".into(), String::new(),
        "| backend | phase | stage | median elapsed ms |".into(), "| --- | --- | --- | ---: |".into(),
    ]).chain(publication_rows).chain([
        String::new(), "## In-process memory by stage (Linux `/proc/self/status`)".into(), String::new(),
        "RSS is sampled at stage boundaries; peak growth is the increase in process high-water RSS since the previous mark.".into(), String::new(),
        "| backend | stage | median boundary RSS MiB | median new peak MiB |".into(), "| --- | --- | ---: | ---: |".into(),
    ]).chain(if resource_rows.is_empty() { vec!["| n/a | no stage samples | n/a | n/a |".into()] } else { resource_rows }).chain([
        String::new(), "## SQL database page accounting (dbstat)".into(), String::new(),
        "Page allocation and payload are retained database pages, not bytes written.".into(), String::new(),
        "| backend | table/index | median page bytes | median payload bytes |".into(), "| --- | --- | ---: | ---: |".into(),
    ]).chain(if storage_rows.is_empty() { vec!["| n/a | dbstat unavailable | n/a | n/a |".into()] } else { storage_rows }).chain(storage_notes).chain([
        String::new(), "Each raw stdout/stderr pair and parsed sample is retained in this run directory. Compilation is warmed outside the timed process trees. Database size is retained storage, not bytes written; this benchmark does not measure write amplification or an SLA.".into(), String::new(),
    ]).collect::<Vec<_>>();
    Ok(lines.join("\n"))
}

fn values_at(samples: &[&Value], path: &[&str]) -> BenchResult<Vec<u64>> {
    samples
        .iter()
        .map(|sample| {
            path.iter()
                .try_fold(*sample, |value, key| {
                    value.get(*key).ok_or("sample metric missing")
                })?
                .as_u64()
                .ok_or_else(|| "sample metric is not an unsigned integer".into())
        })
        .collect()
}

fn values_optional_at(samples: &[&Value], path: &[&str]) -> Vec<u64> {
    let mut values = Vec::new();
    for sample in samples {
        if let Some(value) = path.iter().try_fold(*sample, |value, key| value.get(*key))
            && let Some(value) = value.as_u64()
        {
            values.push(value);
        }
    }
    values
}

fn values_at_profiles(samples: &[&Value], phase: &str, key: &str) -> BenchResult<Vec<u64>> {
    samples
        .iter()
        .map(|sample| {
            sample
                .get("metrics")
                .and_then(|value| value.get("profile_stages"))
                .and_then(|value| value.get(phase))
                .and_then(|value| value.get(key))
                .and_then(Value::as_u64)
                .ok_or_else(|| "sample omitted profile metric".into())
        })
        .collect()
}

fn resource_values(
    samples: &[&Value],
    backend: &str,
    stage: &str,
    field: &str,
) -> BenchResult<Vec<u64>> {
    let mut values = Vec::new();
    for sample in samples {
        for resource in sample
            .get("resource_stages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if resource.get("backend").and_then(Value::as_str) == Some(backend)
                && resource.get("stage").and_then(Value::as_str) == Some(stage)
            {
                values.push(
                    resource
                        .get(field)
                        .and_then(Value::as_u64)
                        .ok_or("resource metric missing")?,
                );
            }
        }
    }
    Ok(values)
}

fn storage_values(samples: &[&Value], name: &str, field: &str) -> Vec<u64> {
    samples
        .iter()
        .flat_map(|sample| {
            sample
                .get("database_breakdown")
                .and_then(|value| value.get("objects"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter(|item| item.get("name").and_then(Value::as_str) == Some(name))
        .filter_map(|item| item.get(field).and_then(Value::as_u64))
        .collect()
}

fn median(values: &[u64]) -> Option<u64> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let midpoint = sorted.len().checked_div(2)?;
    let right = *sorted.get(midpoint)?;
    if sorted.len() % 2 == 1 {
        return Some(right);
    }
    let left_index = midpoint.checked_sub(1)?;
    let left = *sorted.get(left_index)?;
    Some(
        u64::try_from(
            u128::from(left)
                .saturating_add(u128::from(right))
                .div_ceil(2),
        )
        .unwrap_or(u64::MAX),
    )
}

fn comma(value: u64) -> String {
    let text = value.to_string();
    let mut output = String::with_capacity(text.len().saturating_add(text.len() / 3));
    for (index, character) in text.chars().enumerate() {
        if index > 0 && text.len().saturating_sub(index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    output
}

fn scaled(value: u64, divisor: u64, digits: u64) -> String {
    let scale = 10_u64.saturating_pow(u32::try_from(digits).unwrap_or_default());
    let scaled = value
        .saturating_mul(scale)
        .saturating_add(divisor / 2)
        .checked_div(divisor)
        .unwrap_or_default();
    let integer = scaled.checked_div(scale).unwrap_or_default();
    let fraction = scaled.checked_rem(scale).unwrap_or_default();
    format!(
        "{integer}.{fraction:0width$}",
        width = usize::try_from(digits).unwrap_or_default()
    )
}

fn format_milliseconds_microseconds(value: u64) -> String {
    format!(
        "{}.{:03}",
        value.checked_div(1000).unwrap_or_default(),
        value.checked_rem(1000).unwrap_or_default()
    )
}

fn number_to_microseconds(value: &Value) -> Option<u64> {
    let text = value.as_number()?.to_string();
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let whole = whole.parse::<u64>().ok()?;
    let fraction = format!("{fraction:0<3}");
    let fraction = fraction.get(..3)?.parse::<u64>().ok()?;
    whole.checked_mul(1000)?.checked_add(fraction)
}

fn milliseconds_number(microseconds: u64) -> BenchResult<Number> {
    let text = format_milliseconds_microseconds(microseconds);
    text.parse::<Number>().map_err(|error| error.into())
}

fn insert_json(root: &mut Value, parent: &str, key: &str, value: Number) -> BenchResult<()> {
    let object = root
        .get_mut(parent)
        .and_then(Value::as_object_mut)
        .ok_or("publication phase missing")?;
    object.insert(key.to_owned(), Value::Number(value));
    Ok(())
}

fn capture_value(capture: &regex::Captures<'_>, name: &str) -> String {
    capture
        .name(name)
        .map_or_else(String::new, |value| value.as_str().to_owned())
}

fn capture_u64(capture: &regex::Captures<'_>, name: &str) -> BenchResult<u64> {
    capture
        .name(name)
        .ok_or_else(|| format!("benchmark line missing field {name}"))?
        .as_str()
        .parse::<u64>()
        .map_err(|error| error.into())
}

fn write_json(path: &Path, value: &Value) -> BenchResult<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn write_output(prefix: &Path, output: &Output) -> BenchResult<()> {
    fs::write(prefix.with_extension("stdout.txt"), &output.stdout)?;
    fs::write(prefix.with_extension("stderr.txt"), &output.stderr)?;
    Ok(())
}

fn read_time_measurement(path: &Path, exit: std::process::ExitStatus) -> BenchResult<u64> {
    let text = fs::read_to_string(path)?;
    let (rss, reported_exit) = text
        .trim()
        .split_once('\t')
        .ok_or("GNU time output missing fields")?;
    if reported_exit.parse::<i32>()?
        != exit
            .code()
            .ok_or("benchmark exited without numeric status")?
    {
        return Err("GNU time exit status differs from benchmark status".into());
    }
    Ok(rss.parse()?)
}

fn gnu_time_binary() -> Option<PathBuf> {
    let candidate = PathBuf::from("/usr/bin/time");
    let output = Command::new(&candidate).arg("--version").output().ok()?;
    String::from_utf8_lossy(&output.stdout)
        .to_ascii_lowercase()
        .contains("gnu time")
        .then_some(candidate)
}

fn cpu_model() -> Option<String> {
    let contents = fs::read_to_string("/proc/cpuinfo").ok()?;
    contents.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("model name") || lower.starts_with("hardware") {
            line.split_once(':')
                .map(|(_, value)| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        } else {
            None
        }
    })
}

fn git_value(workspace: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn successful_stdout(output: Option<&Output>) -> Option<String> {
    let output = output.filter(|output| output.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn command_output(command: &str, args: &[&str]) -> Option<Output> {
    Command::new(command).args(args).output().ok()
}

fn absolute_path(path: &Path) -> BenchResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn run_child(
    command: &[String],
    cwd: &Path,
    environment: &BTreeMap<std::ffi::OsString, std::ffi::OsString>,
) -> BenchResult<Output> {
    let (program, arguments) = command.split_first().ok_or("empty benchmark command")?;
    Ok(Command::new(program)
        .args(arguments)
        .current_dir(cwd)
        .envs(environment)
        .output()?)
}

fn exit_error(status: std::process::ExitStatus, message: &str) -> BenchResult<()> {
    Err(format!("{message}: {status}").into())
}

#[cfg(test)]
mod tests {
    use super::{
        IN_MEMORY_STORE_DELTA_STAGES, PENELOPE_STAGES, Patterns, STORE_DELTA_STAGES,
        parse_publication,
    };

    fn diagnostics(store_stages: &[&str]) -> String {
        let mut output = String::new();
        for _ in 0..2 {
            for stage in PENELOPE_STAGES {
                output.push_str("penelope_stage stage=");
                output.push_str(stage);
                output.push_str(" elapsed_us=1\n");
            }
            for stage in store_stages {
                output.push_str("store_delta_stage stage=");
                output.push_str(stage);
                output.push_str(" elapsed_us=1\n");
            }
        }
        output
    }

    #[test]
    fn publication_parser_requires_complete_nested_lineage_profile()
    -> Result<(), Box<dyn std::error::Error>> {
        let patterns = Patterns::new()?;
        let publication = parse_publication(
            &diagnostics(&IN_MEMORY_STORE_DELTA_STAGES),
            "in-memory",
            &patterns,
        )?;
        let has_clone = publication
            .get("incremental")
            .and_then(|value| value.get("store_delta.lineage_candidate_clone"))
            .is_some_and(serde_json::Value::is_number);
        let has_publish = publication
            .get("incremental")
            .and_then(|value| value.get("store_delta.lineage_candidate_publish"))
            .is_some_and(serde_json::Value::is_number);
        if !has_clone || !has_publish {
            return Err("publication output omitted nested lineage stages".into());
        }

        let old_inner_only_profile = &STORE_DELTA_STAGES[2..8];
        if parse_publication(&diagnostics(old_inner_only_profile), "in-memory", &patterns).is_ok() {
            return Err("parser accepted incomplete nested lineage stages".into());
        }
        Ok(())
    }
}
