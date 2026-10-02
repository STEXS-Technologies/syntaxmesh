use std::error::Error;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde_json::Value;
use syntaxmesh_store_turso::TursoGraphStore;

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _killed = self.0.kill();
        let _waited = self.0.wait();
    }
}

fn start(root: &Path, database: &Path, polling: bool) -> Result<(Process, String), Box<dyn Error>> {
    start_with_mcp(root, database, polling, false)
}

fn start_with_mcp(
    root: &Path,
    database: &Path,
    polling: bool,
    mcp: bool,
) -> Result<(Process, String), Box<dyn Error>> {
    start_with_policy(root, database, polling, mcp, true)
}

fn start_with_policy(
    root: &Path,
    database: &Path,
    polling: bool,
    mcp: bool,
    verify: bool,
) -> Result<(Process, String), Box<dyn Error>> {
    start_with_policy_duration(root, database, polling, mcp, verify, Some("5000"))
}

fn start_with_policy_duration(
    root: &Path,
    database: &Path,
    polling: bool,
    mcp: bool,
    verify: bool,
    duration_ms: Option<&str>,
) -> Result<(Process, String), Box<dyn Error>> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_syntaxmeshd"));
    command
        .arg(root)
        .arg(database)
        .arg("127.0.0.1:0")
        .args(["--reconcile-ms", "100"])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(duration) = duration_ms {
        command.args(["--watch-duration-ms", duration]);
    }
    if verify {
        command.arg("--verify");
    }
    if polling {
        command.arg("--poll-only");
    }
    if mcp {
        command.args(["--mcp", "--context-tokenizer", "cl100k_base"]);
    }
    let mut process = Process(command.spawn()?);
    let stdout = process.0.stdout.take().ok_or("daemon stdout unavailable")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout)
            .read_line(&mut line)
            .map(|_length| line);
        let _sent = sender.send(result);
    });
    let line = receiver.recv_timeout(Duration::from_secs(10))??;
    reader
        .join()
        .map_err(|_panic| "readiness reader panicked")?;
    let address = line
        .trim()
        .strip_prefix("syntaxmeshd listening on ")
        .ok_or("daemon did not report readiness")?;
    Ok((process, format!("http://{address}")))
}

#[path = "lifecycle/mcp.rs"]
mod mcp;

#[path = "lifecycle/config_reload.rs"]
mod config_reload;
#[path = "lifecycle/discovery.rs"]
mod discovery;

fn search(
    client: &Client,
    base: &str,
    symbol: &str,
    generation: Option<&str>,
) -> Result<Value, Box<dyn Error>> {
    let mut request = client
        .get(format!("{base}/api/v1/search"))
        .query(&[("text", symbol)]);
    if let Some(generation) = generation {
        request = request.query(&[("generation", generation)]);
    }
    Ok(request.send()?.error_for_status()?.json()?)
}

fn generation(response: &Value) -> Result<&str, Box<dyn Error>> {
    response
        .get("generation")
        .and_then(Value::as_str)
        .ok_or_else(|| "generation missing".into())
}

fn has_nodes(response: &Value) -> bool {
    response
        .get("data")
        .and_then(Value::as_array)
        .is_some_and(|nodes| !nodes.is_empty())
}

fn wait_exit(process: &mut Process) -> Result<(), Box<dyn Error>> {
    wait_status(process, true)
}

fn wait_status(process: &mut Process, expected_success: bool) -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    loop {
        if let Some(status) = process.0.try_wait()? {
            if status.success() != expected_success {
                return Err(format!("unexpected daemon exit: {status}").into());
            }
            return Ok(());
        }
        if started.elapsed() > Duration::from_secs(10) {
            return Err("daemon did not stop".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn audit_history(root: &Path, database: &Path) -> Result<(), Box<dyn Error>> {
    let _ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let canonical_root = root.canonicalize()?;
    let config = syntaxmesh_source_host::ProjectConfig::load(&canonical_root)?;
    let mut setup = syntaxmesh_source_host::configured_source_engine(
        TursoGraphStore::open(database)?,
        &canonical_root,
        &config,
        false,
    )?;
    if setup.engine.verify_statechronicle_history()?.is_none() {
        return Err("daemon restart lost verified generation history".into());
    }
    Ok(())
}

#[test]
fn abrupt_death_releases_lease_and_restart_reconciles_offline_edits() -> Result<(), Box<dyn Error>>
{
    for polling in [false, true] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        std::fs::create_dir(&root)?;
        let file = root.join("lib.rs");
        std::fs::write(&file, "pub fn crash_before() {}")?;
        let database = fixture.path().join("graph.db");
        TursoGraphStore::migrate(&database)?;
        let (mut process, base) = start(&root, &database, polling)?;
        if !syntaxmesh_ownership_host::is_writer_active(&database)? {
            return Err("ownership probe missed the daemon process".into());
        }
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()?;
        let initial = search(&client, &base, "crash_before", None)?;
        if !has_nodes(&initial) {
            return Err("initial crash fixture source missing".into());
        }
        let old = generation(&initial)?.to_owned();
        process.0.kill()?;
        wait_status(&mut process, false)?;
        if syntaxmesh_ownership_host::is_writer_active(&database)? {
            return Err("ownership probe retained an abruptly killed daemon".into());
        }
        // Keep the persistent lease sidecar: OS process death releases its lock.
        audit_history(&root, &database)?;
        std::fs::write(&file, "pub fn crash_after() {}")?;
        let (mut restarted, restarted_base) = start(&root, &database, polling)?;
        let current = search(&client, &restarted_base, "crash_after", None)?;
        if !has_nodes(&current)
            || generation(&current)? == old
            || has_nodes(&search(&client, &restarted_base, "crash_before", None)?)
            || !has_nodes(&search(
                &client,
                &restarted_base,
                "crash_before",
                Some(&old),
            )?)
        {
            return Err("crash restart lost retained history or offline source edit".into());
        }
        #[cfg(unix)]
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(i32::try_from(restarted.0.id())?),
            nix::sys::signal::Signal::SIGTERM,
        )?;
        wait_exit(&mut restarted)?;
        audit_history(&root, &database)?;
    }
    Ok(())
}

#[test]
fn failed_reconciliation_stops_daemon_and_repaired_restart_is_a_noop() -> Result<(), Box<dyn Error>>
{
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir(&root)?;
    let file = root.join("lib.rs");
    let source = "pub fn repaired_source() {}";
    std::fs::write(&file, source)?;
    let database = fixture.path().join("graph.db");
    TursoGraphStore::migrate(&database)?;
    let (mut process, base) = start(&root, &database, true)?;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()?;
    let initial = search(&client, &base, "repaired_source", None)?;
    if !has_nodes(&initial) {
        return Err("repair fixture initial source missing".into());
    }
    let old = generation(&initial)?.to_owned();
    std::fs::write(&file, [255_u8])?;
    wait_status(&mut process, false)?;
    audit_history(&root, &database)?;
    std::fs::write(&file, source)?;
    let (mut restarted, restarted_base) = start(&root, &database, true)?;
    let repaired = search(&client, &restarted_base, "repaired_source", None)?;
    if !has_nodes(&repaired) || generation(&repaired)? != old {
        return Err("repaired unchanged source published duplicate generation".into());
    }
    #[cfg(unix)]
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(i32::try_from(restarted.0.id())?),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    wait_exit(&mut restarted)?;
    audit_history(&root, &database)?;
    Ok(())
}

#[test]
fn daemon_process_updates_live_reads_retains_history_and_releases_ownership()
-> Result<(), Box<dyn Error>> {
    for polling in [false, true] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        std::fs::create_dir(&root)?;
        let file = root.join("lib.rs");
        std::fs::write(&file, "pub fn daemon_before() {}")?;
        std::fs::write(
            root.join("design.md"),
            "# Initial architecture\nRust owns execution.\n",
        )?;
        let database = fixture.path().join("graph.db");
        TursoGraphStore::migrate(&database)?;
        let (mut process, base) = start(&root, &database, polling)?;
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()?;
        let initial = search(&client, &base, "daemon_before", None)?;
        if !has_nodes(&initial) {
            return Err("initial daemon source missing".into());
        }
        let old = generation(&initial)?.to_owned();
        let competing = Command::new(env!("CARGO_BIN_EXE_syntaxmeshd"))
            .arg(&root)
            .arg(&database)
            .arg("127.0.0.1:0")
            .output()?;
        if competing.status.success()
            || !String::from_utf8_lossy(&competing.stderr).contains("AlreadyOwned")
        {
            return Err("competing daemon was not rejected by shared lease".into());
        }
        std::fs::write(&file, "pub fn daemon_after() {}")?;
        let waiting = Instant::now();
        let current = loop {
            let response = search(&client, &base, "daemon_after", None)?;
            if has_nodes(&response) && generation(&response)? != old {
                break generation(&response)?.to_owned();
            }
            if waiting.elapsed() > Duration::from_secs(3) {
                return Err("daemon did not publish saved source".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        std::thread::sleep(Duration::from_millis(250));
        if generation(&search(&client, &base, "daemon_after", None)?)? != current
            || has_nodes(&search(&client, &base, "daemon_before", None)?)
            || !has_nodes(&search(&client, &base, "daemon_before", Some(&old))?)
        {
            return Err("daemon no-op/current/history invariants failed".into());
        }
        #[cfg(unix)]
        {
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(i32::try_from(process.0.id())?),
                nix::sys::signal::Signal::SIGTERM,
            )?;
        }
        wait_exit(&mut process)?;
        if syntaxmesh_ownership_host::is_writer_active(&database)? {
            return Err("ownership probe retained a terminated daemon".into());
        }
        let (mut restarted, restarted_base) = start(&root, &database, polling)?;
        if generation(&search(&client, &restarted_base, "daemon_after", None)?)? != current
            || !has_nodes(&search(
                &client,
                &restarted_base,
                "daemon_before",
                Some(&old),
            )?)
        {
            return Err("restart did not retain current and historical generations".into());
        }
        #[cfg(unix)]
        {
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(i32::try_from(restarted.0.id())?),
                nix::sys::signal::Signal::SIGTERM,
            )?;
        }
        wait_exit(&mut restarted)?;
    }
    Ok(())
}

#[test]
fn invalid_bind_or_missing_database_does_not_create_storage() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let missing = fixture.path().join("missing.db");
    for address in ["0.0.0.0:0", "127.0.0.1:0"] {
        let output = Command::new(env!("CARGO_BIN_EXE_syntaxmeshd"))
            .arg(fixture.path())
            .arg(&missing)
            .arg(address)
            .output()?;
        if output.status.success() || missing.exists() {
            return Err("invalid daemon startup created storage or succeeded".into());
        }
    }
    Ok(())
}

#[test]
fn inside_root_and_unmigrated_databases_are_rejected_without_overwrite()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir(&root)?;
    let inside = root.join("graph.db");
    TursoGraphStore::migrate(&inside)?;
    let unmigrated = fixture.path().join("unmigrated.db");
    std::fs::write(&unmigrated, b"not a migrated database")?;
    for database in [&inside, &unmigrated] {
        let before = std::fs::read(database)?;
        let output = Command::new(env!("CARGO_BIN_EXE_syntaxmeshd"))
            .arg(&root)
            .arg(database)
            .arg("127.0.0.1:0")
            .output()?;
        if output.status.success() || std::fs::read(database)? != before {
            return Err("invalid database was accepted or overwritten".into());
        }
    }
    Ok(())
}
