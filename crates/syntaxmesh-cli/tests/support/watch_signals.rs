use super::*;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

#[test]
fn termination_and_process_death_release_idle_watch_ownership() -> Result<(), Box<dyn Error>> {
    for turso in [false, true] {
        for signal in [Signal::SIGTERM, Signal::SIGHUP, Signal::SIGKILL] {
            let fixture = tempfile::tempdir()?;
            let root = fixture.path().join("source");
            fs::create_dir(&root)?;
            fs::write(root.join("lib.rs"), "pub fn signal_retained_symbol() {}\n")?;
            let store = fixture
                .path()
                .join(if turso { "graph.db" } else { "graph.snapshot" });
            if turso {
                syntaxmesh_store_turso::TursoGraphStore::migrate(&store)?;
            }
            let log = fixture.path().join("watch.log");
            let mut child = WatchedChild(
                Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                    .arg(if turso { "watch-turso" } else { "watch" })
                    .arg(&root)
                    .arg(&store)
                    .arg("--verify")
                    .stdout(Stdio::from(fs::File::create(&log)?))
                    .stderr(Stdio::from(fs::File::create(
                        fixture.path().join("watch.err"),
                    )?))
                    .spawn()?,
            );
            wait_log(&log, "indexed 1 files", 1)?;
            let raw_pid = i32::try_from(child.0.id())?;
            if raw_pid <= 0 {
                return Err("invalid watch child process ID".into());
            }
            kill(Pid::from_raw(raw_pid), signal)?;
            let started = Instant::now();
            loop {
                if let Some(status) = child.0.try_wait()? {
                    if status.success() == (signal == Signal::SIGKILL) {
                        return Err(
                            format!("unexpected watch exit for {signal:?}: {status}").into()
                        );
                    }
                    break;
                }
                if started.elapsed() > Duration::from_secs(5) {
                    return Err("signalled watch failed to exit".into());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let index = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(if turso { "index-turso" } else { "index" })
                .arg(&root)
                .arg(&store)
                .arg("--verify")
                .output()?;
            if !index.status.success() {
                return Err(format!(
                    "process exit did not release ownership: {}",
                    String::from_utf8(index.stderr)?
                )
                .into());
            }
            wait_search(&store, turso, "signal_retained_symbol", true)?;
            let audit = Command::new(env!("CARGO_BIN_EXE_syntaxmesh"))
                .arg(if turso {
                    "statechronicle-verify-turso"
                } else {
                    "statechronicle-verify"
                })
                .arg(&store)
                .output()?;
            if !audit.status.success()
                || !String::from_utf8(audit.stdout)?.contains("statechronicle_history=verified")
            {
                return Err("process exit lost verified generation history".into());
            }
        }
    }
    Ok(())
}
