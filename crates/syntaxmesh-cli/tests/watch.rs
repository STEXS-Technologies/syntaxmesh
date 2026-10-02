use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
#[path = "support/watch_signals.rs"]
mod signals;

struct WatchedChild(Child);

impl Drop for WatchedChild {
    fn drop(&mut self) {
        let _kill = self.0.kill();
        let _wait = self.0.wait();
    }
}

fn wait_search(
    store: &Path,
    turso: bool,
    name: &str,
    expected: bool,
) -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        let output = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(if turso { "search-turso" } else { "search" })
            .arg(store)
            .arg(name)
            .output()?;
        if output.status.success() && String::from_utf8(output.stdout)?.contains(name) == expected {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(format!("watch did not reach expected search state for {name}").into())
}

fn wait_log(path: &Path, needle: &str, count: usize) -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        if fs::read_to_string(path)?.matches(needle).count() >= count {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(format!("watch log did not contain {count} occurrences of {needle}").into())
}

#[test]
fn watch_updates_removals_and_excludes_competing_writers() -> Result<(), Box<dyn Error>> {
    for (turso, poll_only) in [(false, false), (true, false), (false, true), (true, true)] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        fs::create_dir(&root)?;
        let source = root.join("lib.rs");
        fs::write(&source, "pub fn initial_watch_symbol() {}\n")?;
        let store = fixture
            .path()
            .join(if turso { "graph.db" } else { "graph.snapshot" });
        if turso {
            syntaxmesh_store_turso::TursoGraphStore::migrate(&store)?;
        }
        let log = fixture.path().join("watch.log");
        let stdout = fs::File::create(&log)?;
        let stderr = fs::File::create(fixture.path().join("watch.err"))?;
        let mut child = WatchedChild(
            Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(if turso { "watch-turso" } else { "watch" })
                .arg(&root)
                .arg(&store)
                .args(poll_only.then_some("--poll-only"))
                .args([
                    "--verify",
                    "--reconcile-ms",
                    "200",
                    "--watch-duration-ms",
                    "4000",
                ])
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn()?,
        );
        wait_log(&log, "indexed 1 files into generation", 1).map_err(|error| {
            format!(
                "{error}; turso={turso}; stdout={}; stderr={}",
                fs::read_to_string(&log).unwrap_or_default(),
                fs::read_to_string(fixture.path().join("watch.err")).unwrap_or_default()
            )
        })?;
        let initial_log = fs::read_to_string(&log)?;
        let initial_generation = initial_log
            .lines()
            .find(|line| line.starts_with("indexed 1 files"))
            .and_then(|line| line.split_whitespace().nth(5))
            .ok_or("initial generation is missing")?
            .to_owned();
        let blocked = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(if turso { "index-turso" } else { "index" })
            .arg(&root)
            .arg(&store)
            .output()?;
        if blocked.status.success() || !String::from_utf8(blocked.stderr)?.contains("already owned")
        {
            return Err("watch did not retain writer ownership".into());
        }
        fs::write(&source, "pub fn changed_watch_symbol() {}\n")?;
        wait_log(&log, "indexed 1 files into generation", 2)?;
        if !turso {
            wait_search(&store, turso, "changed_watch_symbol", true)?;
            wait_search(&store, turso, "initial_watch_symbol", false)?;
        }
        fs::remove_file(&source)?;
        wait_log(&log, "indexed 0 files into generation", 1)?;
        let started = Instant::now();
        loop {
            if let Some(status) = child.0.try_wait()? {
                if !status.success() {
                    return Err(format!(
                        "watch failed: {}",
                        fs::read_to_string(fixture.path().join("watch.err"))?
                    )
                    .into());
                }
                break;
            }
            if started.elapsed() > Duration::from_secs(5) {
                return Err("watch duration did not stop host".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let completed_log = fs::read_to_string(&log)?;
        if completed_log.matches("files into generation").count() != 3
            || !completed_log.contains("already up to date")
        {
            return Err(
                "periodic watch reconciliation published unchanged sources or did not run".into(),
            );
        }
        wait_search(&store, turso, "changed_watch_symbol", false)?;
        let historical = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(if turso { "graph-at-turso" } else { "graph-at" })
            .arg(&store)
            .arg(&initial_generation)
            .output()?;
        if !historical.status.success()
            || !String::from_utf8(historical.stdout)?.contains("initial_watch_symbol")
        {
            return Err("watch discarded historical source facts".into());
        }
        let accepted = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
            .arg(if turso { "index-turso" } else { "index" })
            .arg(&root)
            .arg(&store)
            .arg("--verify")
            .output()?;
        if !accepted.status.success() {
            return Err("watch shutdown did not release ownership".into());
        }
    }
    Ok(())
}
