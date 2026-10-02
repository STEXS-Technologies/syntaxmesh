//! Rust-native runner for the Linux database write-syscall benchmark.

use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use chrono::Utc;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::{Value, json};

type BenchResult<T> = Result<T, Box<dyn Error>>;
const STAGES: [&str; 3] = ["bootstrap", "initial", "incremental"];
const BACKENDS: [&str; 2] = ["sqlite", "turso"];
const WRITE_SYSCALLS: &str = "write,pwrite64,writev,pwritev,pwritev2";

pub(super) fn run(workspace: &Path) -> BenchResult<()> {
    if !cfg!(target_os = "linux") {
        return Err("file-write syscall measurement currently requires Linux".into());
    }
    let arguments: Vec<String> = env::args().skip(2).collect();
    if arguments.len() > 1 {
        return Err("usage: cargo make benchmark-file-writes [OUTPUT_ROOT]".into());
    }
    let strace = find_program("strace")
        .ok_or("strace is required (install it with the system package manager)")?;
    let output_root = arguments
        .first()
        .map_or_else(|| workspace.join("target/benchmark-results"), PathBuf::from);
    let output_root = absolute_path(&output_root)?;
    fs::create_dir_all(&output_root)?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.6fZ").to_string();
    let revision = git_value(workspace, &["rev-parse", "--short=12", "HEAD"]);
    let run_dir = output_root.join(format!(
        "write-syscalls-{timestamp}-{}",
        revision.as_deref().unwrap_or("uncommitted")
    ));
    if run_dir.exists() {
        return Err(format!("refusing to overwrite benchmark run: {}", run_dir.display()).into());
    }
    let trace_dir = run_dir.join("traces");
    let result_dir = run_dir.join("results");
    fs::create_dir_all(&trace_dir)?;
    fs::create_dir_all(&result_dir)?;

    let temporary_root = env::var_os("SYNTAXMESH_BENCH_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target/benchmark-write-databases"));
    let temporary_root = absolute_path(&temporary_root)?;
    fs::create_dir_all(&temporary_root)?;
    let temporary_path =
        temporary_root.join(format!("write-database-{}-{timestamp}", std::process::id()));
    fs::create_dir(&temporary_path)?;
    let _temporary = TemporaryDirectory(temporary_path.clone());
    let database_dir = temporary_path.join("database");
    fs::create_dir(&database_dir)?;
    let sqlite_path = database_dir.join("repository.db");
    let turso_path = database_dir.join("repository-turso.db");

    let rustc = command_output("rustc", &["--version"]);
    let strace_version = Command::new(&strace).arg("--version").output().ok();
    let fixture_root = env::var_os("SYNTAXMESH_BENCH_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.to_owned());
    let metadata = json!({
        "schema_version": 1,
        "revision": revision,
        "working_tree_dirty": git_value(workspace, &["status", "--porcelain"]).is_some_and(|value| !value.is_empty()),
        "timestamp_utc": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        "fixture_root": absolute_path(&fixture_root)?.display().to_string(),
        "platform": format!("{}-{}", env::consts::OS, env::consts::ARCH),
        "machine": env::consts::ARCH,
        "logical_cpus": std::thread::available_parallelism().ok().map(usize::from),
        "rustc": successful_stdout(rustc.as_ref()),
        "strace": strace_version.as_ref().and_then(|result| String::from_utf8_lossy(&result.stdout).lines().next().map(str::to_owned)),
        "command": ["cargo", "make", "benchmark-repository"],
        "metric": "successful database-file write-family syscall bytes",
        "limitations": ["not physical device I/O", "mmap dirty-page writeback is omitted", "strace materially distorts timings", "bootstrap writes are measured separately and are not incremental write amplification"]
    });
    write_json(&run_dir.join("metadata.json"), &metadata)?;

    let mut stages = serde_json::Map::new();
    let mut byte_totals = BTreeMap::<String, BTreeMap<String, u64>>::new();
    let mut call_totals = BTreeMap::<String, u64>::new();
    for stage in STAGES {
        for backend in BACKENDS {
            let trace_path = trace_dir.join(format!("{stage}-{backend}.strace"));
            let command = strace_command(&strace, &trace_path, &sqlite_path, &turso_path);
            let mut environment = env::vars_os().collect::<BTreeMap<_, _>>();
            // Turso uses the process temp directory for internal scratch files,
            // not only the database path supplied by the benchmark. Keep that
            // traffic with this run so a full shared /tmp quota cannot break a
            // traced publication midway through the fixture.
            environment.insert("TMPDIR".into(), temporary_path.as_os_str().to_owned());
            environment.insert(
                "SYNTAXMESH_BENCH_DB_DIR".into(),
                database_dir.as_os_str().to_owned(),
            );
            environment
                .entry("SYNTAXMESH_BENCH_ROOT".into())
                .or_insert_with(|| workspace.as_os_str().to_owned());
            environment.insert("SYNTAXMESH_BENCH_KEEP".into(), "1".into());
            environment.insert("SYNTAXMESH_BENCH_STAGE".into(), stage.into());
            environment.insert("SYNTAXMESH_BENCH_BACKEND".into(), backend.into());
            let started = Instant::now();
            let output = run_command(&command, workspace, &environment)?;
            let elapsed = started.elapsed().as_secs_f64();
            let prefix = result_dir.join(format!("{stage}-{backend}"));
            fs::write(prefix.with_extension("stdout.txt"), &output.stdout)?;
            fs::write(prefix.with_extension("stderr.txt"), &output.stderr)?;
            if !output.status.success() {
                write_json(
                    &run_dir.join("results.json"),
                    &json!({"schema_version":1,"stages":stages}),
                )?;
                eprintln!("benchmark_evidence={}", run_dir.display());
                return Err(
                    format!("benchmark {stage}/{backend} exited with {}", output.status).into(),
                );
            }
            let trace = fs::read_to_string(&trace_path).unwrap_or_default();
            let (file_bytes, file_calls) = parse_trace(&trace, &database_dir);
            let total_bytes = file_bytes.values().sum::<u64>();
            let total_calls = file_calls.values().sum::<u64>();
            let stage_calls = call_totals.entry(stage.to_owned()).or_default();
            *stage_calls = stage_calls.saturating_add(total_calls);
            byte_totals
                .entry(stage.to_owned())
                .or_default()
                .insert(backend.to_owned(), total_bytes);
            let database = if backend == "sqlite" {
                &sqlite_path
            } else {
                &turso_path
            };
            let history_bytes = history_payload_bytes(database).ok().flatten().unwrap_or(0);
            let metric = stage_metrics(&output.stdout);
            let mut item = json!({
                "successful_file_write_syscall_bytes": total_bytes,
                "successful_file_write_syscall_calls": total_calls,
                "successful_file_write_syscall_bytes_by_file": file_bytes,
                "successful_file_write_syscall_calls_by_file": file_calls,
                "elapsed_seconds_including_tracing": elapsed,
                "generation_history_payload_bytes": history_bytes,
            });
            if let Some(metric) = metric
                && let Some(object) = item.as_object_mut()
            {
                object.insert("index".into(), metric);
            }
            stages
                .entry(stage.to_owned())
                .or_insert_with(|| Value::Object(serde_json::Map::new()))[backend] = item;
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
    }
    let results = json!({"schema_version":1,"stages":stages});
    println!("metric=successful_database_file_write_syscall_bytes (not physical device I/O)");
    println!(
        "limitation=mmap dirty-page writeback is not counted; kernel/device caching is not measured"
    );
    if byte_totals
        .values()
        .flat_map(BTreeMap::values)
        .all(|value| *value == 0)
    {
        write_json(&run_dir.join("results.json"), &results)?;
        eprintln!("error: no writes to benchmark SQLite/Turso database or WAL files were observed");
        eprintln!("benchmark_evidence={}", run_dir.display());
        return Err("no matching database writes observed".into());
    }
    for stage in STAGES {
        println!(
            "stage={stage} matched_write_syscalls={}",
            call_totals.get(stage).copied().unwrap_or_default()
        );
        for backend in BACKENDS {
            println!(
                "stage={stage} backend={backend} successful_file_write_syscall_bytes={}",
                byte_totals
                    .get(stage)
                    .and_then(|by_backend| by_backend.get(backend))
                    .copied()
                    .unwrap_or_default()
            );
        }
    }
    write_json(&run_dir.join("results.json"), &results)?;
    write_summary(&run_dir, &metadata, &results)?;
    println!("benchmark_evidence={}", run_dir.display());
    Ok(())
}

struct TemporaryDirectory(PathBuf);

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn strace_command(strace: &Path, trace: &Path, sqlite: &Path, turso: &Path) -> Vec<String> {
    let mut command = vec![
        strace.display().to_string(),
        "-f".into(),
        "-qq".into(),
        "-yy".into(),
        "-s".into(),
        "0".into(),
    ];
    for database in [sqlite, turso] {
        command.extend(["-P".into(), database.display().to_string()]);
        command.extend(["-P".into(), format!("{}-wal", database.display())]);
        command.extend(["-P".into(), format!("{}-shm", database.display())]);
    }
    command.extend([
        "-e".into(),
        format!("trace={WRITE_SYSCALLS}"),
        "-o".into(),
        trace.display().to_string(),
        "--".into(),
        "cargo".into(),
        "make".into(),
        "benchmark-repository".into(),
    ]);
    command
}

fn parse_trace(trace: &str, database_dir: &Path) -> (BTreeMap<String, u64>, BTreeMap<String, u64>) {
    let mut bytes = BTreeMap::<String, u64>::new();
    let mut calls = BTreeMap::<String, u64>::new();
    for line in trace.lines() {
        let Some((call, tail)) = line.split_once('(') else {
            continue;
        };
        let syscall = call.split_whitespace().last().unwrap_or_default();
        if !["write", "pwrite64", "writev", "pwritev", "pwritev2"].contains(&syscall) {
            continue;
        }
        let Some((_, path_tail)) = line.split_once('<') else {
            continue;
        };
        let Some((path_text, _)) = path_tail.split_once('>') else {
            continue;
        };
        let path = Path::new(path_text);
        if path.parent() != Some(database_dir) {
            continue;
        }
        let Some(result) = tail.rsplit_once('=') else {
            continue;
        };
        let value = result.1.split_whitespace().next().unwrap_or_default();
        let Ok(value) = value.parse::<u64>() else {
            continue;
        };
        let kind = if path.to_string_lossy().ends_with("-wal") {
            "wal"
        } else if path.to_string_lossy().ends_with("-shm") {
            "shm"
        } else {
            "database"
        };
        let file_bytes = bytes.entry(kind.to_owned()).or_default();
        *file_bytes = file_bytes.saturating_add(value);
        let file_calls = calls.entry(kind.to_owned()).or_default();
        *file_calls = file_calls.saturating_add(1);
    }
    (bytes, calls)
}

fn stage_metrics(stdout: &[u8]) -> Option<Value> {
    let pattern = regex::Regex::new(r"backend=(?P<backend>[\w-]+) stage=(?P<stage>[\w-]+) index_ms=(?P<elapsed>\d+) logical_input_bytes=(?P<input>\d+) files=(?P<files>\d+) nodes=(?P<nodes>\d+) edges=(?P<edges>\d+) provenance=(?P<provenance>\d+)").ok()?;
    let stdout = String::from_utf8_lossy(stdout);
    let captures = pattern.captures(&stdout)?;
    Some(
        json!({"elapsed_ms_under_strace": captures.name("elapsed")?.as_str().parse::<u64>().ok()?, "logical_input_bytes": captures.name("input")?.as_str().parse::<u64>().ok()?, "files": captures.name("files")?.as_str().parse::<u64>().ok()?, "nodes": captures.name("nodes")?.as_str().parse::<u64>().ok()?, "edges": captures.name("edges")?.as_str().parse::<u64>().ok()?, "provenance": captures.name("provenance")?.as_str().parse::<u64>().ok()?}),
    )
}

fn history_payload_bytes(database: &Path) -> rusqlite::Result<Option<u64>> {
    if !database.is_file() {
        return Ok(None);
    }
    let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let length = connection.query_row("SELECT length(payload) FROM syntaxmesh_generation_history ORDER BY sequence DESC LIMIT 1", [], |row| row.get::<_, i64>(0)).optional()?;
    Ok(length.and_then(|value| u64::try_from(value).ok()))
}

fn write_summary(directory: &Path, metadata: &Value, results: &Value) -> BenchResult<()> {
    let mut summary = vec!["# SyntaxMesh database write-syscall benchmark".to_owned(), String::new(), format!("- measured at: `{}`", metadata["timestamp_utc"].as_str().unwrap_or("unknown")), format!("- source: `{}`", metadata["revision"].as_str().unwrap_or("uncommitted")), format!("- host: {} ({}) logical CPUs", metadata["platform"].as_str().unwrap_or("unknown"), metadata["logical_cpus"].as_u64().map_or_else(|| "unknown".to_owned(), |n| n.to_string())), format!("- toolchain: `{}`", metadata["rustc"].as_str().unwrap_or("unknown")), "- metric: successful write-family syscalls targeting each database/WAL/SHM path".into(), "- limitations: not physical I/O; mmap writeback omitted; ptrace distorts timings".into(), String::new(), "| stage | backend | source input bytes | latest history payload bytes | DB/WAL/SHM write bytes | total bytes | calls | syscall bytes / history byte |".into(), "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |".into()];
    for stage in STAGES {
        for backend in BACKENDS {
            let item = results
                .get("stages")
                .and_then(|stages| stages.get(stage))
                .and_then(|stage| stage.get(backend))
                .ok_or("benchmark results are missing a stage/backend pair")?;
            let history = item
                .get("generation_history_payload_bytes")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let total = item
                .get("successful_file_write_syscall_bytes")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let calls = item
                .get("successful_file_write_syscall_calls")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let files = item
                .get("successful_file_write_syscall_bytes_by_file")
                .ok_or("benchmark results omitted per-file bytes")?;
            let file_bytes = ["database", "wal", "shm"]
                .map(|kind| {
                    files
                        .get(kind)
                        .and_then(Value::as_u64)
                        .unwrap_or_default()
                        .to_string()
                })
                .join("/");
            let input = item
                .get("index")
                .and_then(|index| index.get("logical_input_bytes"))
                .and_then(Value::as_u64)
                .map_or_else(|| "n/a".to_owned(), |value| value.to_string());
            let ratio = if history > 0 {
                let scaled_ratio = u128::from(total)
                    .saturating_mul(10)
                    .checked_div(u128::from(history))
                    .unwrap_or_default();
                format!("{}.{:01}", scaled_ratio / 10, scaled_ratio % 10)
            } else {
                "n/a".into()
            };
            summary.push(format!("| {stage} | {backend} | {input} | {history} | {file_bytes} | {total} | {calls} | {ratio} |"));
        }
    }
    summary.extend([String::new(), "The history payload is the serialized latest `GenerationHistoryEntry` (manifest plus canonical delta; bootstrap has none). The ratio is successful file-write syscall bytes per history payload byte, not total write amplification: it excludes mmap writeback, indexes/page overhead, device I/O, and other persisted records.".into(), String::new()]);
    fs::write(directory.join("summary.md"), summary.join("\n"))?;
    Ok(())
}

fn write_json(path: &Path, value: &Value) -> BenchResult<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn find_program(program: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|path| path.join(program))
        .find(|path| path.is_file())
}

fn absolute_path(path: &Path) -> BenchResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn git_value(workspace: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn command_output(program: &str, arguments: &[&str]) -> Option<Output> {
    Command::new(program).args(arguments).output().ok()
}

fn successful_stdout(output: Option<&Output>) -> Option<String> {
    let output = output.filter(|output| output.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run_command(
    command: &[String],
    cwd: &Path,
    environment: &BTreeMap<std::ffi::OsString, std::ffi::OsString>,
) -> BenchResult<Output> {
    let (program, arguments) = command.split_first().ok_or("empty command")?;
    let output = Command::new(program)
        .args(arguments)
        .current_dir(cwd)
        .envs(environment)
        .output()?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{parse_trace, stage_metrics};
    use std::path::Path;

    #[test]
    fn trace_parser_counts_only_successful_database_writes_in_target_directory() {
        let trace = concat!(
            "write(3</tmp/db/repository.db>, \"x\", 17) = 17\n",
            "pwrite64(4</tmp/db/repository.db-wal>, \"x\", 9, 0) = 9\n",
            "write(5</tmp/other/repository.db>, \"x\", 99) = 99\n",
            "write(3</tmp/db/repository.db>, \"x\", 17) = -1 ENOSPC (No space left on device)\n",
            "read(3</tmp/db/repository.db>, \"x\", 1) = 1\n",
        );
        let (bytes, calls) = parse_trace(trace, Path::new("/tmp/db"));
        assert_eq!(bytes.get("database"), Some(&17));
        assert_eq!(bytes.get("wal"), Some(&9));
        assert_eq!(calls.values().sum::<u64>(), 2);
    }

    #[test]
    fn stage_metric_parser_extracts_benchmark_dimensions() {
        let metrics = stage_metrics(b"backend=sqlite stage=initial index_ms=12 logical_input_bytes=345 files=6 nodes=7 edges=8 provenance=9");
        assert_eq!(
            metrics.and_then(|value| value
                .get("logical_input_bytes")
                .and_then(serde_json::Value::as_u64)),
            Some(345)
        );
    }
}
