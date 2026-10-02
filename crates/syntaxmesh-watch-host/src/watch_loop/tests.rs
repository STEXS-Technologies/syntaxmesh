use super::*;
use std::time::Duration;

#[test]
fn callback_failure_is_preserved_and_stopping_prevents_later_passes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let stopped = AtomicBool::new(false);
    let options = WatchLoopOptions {
        poll_only: true,
        ..WatchLoopOptions::default()
    };
    let failed = run_watch(fixture.path(), &options, &stopped, || {
        Err::<(), _>("pass failed")
    });
    if !matches!(failed, Err(WatchRunError::Pass("pass failed"))) {
        return Err("watch erased the caller error".into());
    }
    let mut passes = 0;
    run_watch(fixture.path(), &options, &stopped, || {
        passes += 1;
        stopped.store(true, Ordering::Release);
        Ok::<(), std::io::Error>(())
    })?;
    if passes != 1 {
        return Err("stop admitted another pass".into());
    }
    Ok(())
}

#[test]
fn invalid_configuration_does_not_invoke_callback() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    for options in [
        WatchLoopOptions {
            reconcile: Duration::ZERO,
            ..WatchLoopOptions::default()
        },
        WatchLoopOptions {
            maximum_wait: Duration::ZERO,
            ..WatchLoopOptions::default()
        },
        WatchLoopOptions {
            duration: Some(Duration::ZERO),
            ..WatchLoopOptions::default()
        },
        WatchLoopOptions {
            quiet: Duration::ZERO,
            ..WatchLoopOptions::default()
        },
    ] {
        let mut called = false;
        let result = run_watch(fixture.path(), &options, &AtomicBool::new(false), || {
            called = true;
            Ok::<(), std::io::Error>(())
        });
        if called || !matches!(result, Err(WatchRunError::Host(_))) {
            return Err("invalid watch options admitted work".into());
        }
    }
    Ok(())
}
