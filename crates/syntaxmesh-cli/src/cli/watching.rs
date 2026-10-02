use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use syntaxmesh_watch_host::{WatchLoopOptions, WatchRunError, run_watch};

use super::{CliError, indexing, ownership};

pub(super) fn run(
    mut arguments: impl Iterator<Item = String>,
    turso: bool,
) -> Result<(), CliError> {
    let root = arguments
        .next()
        .ok_or_else(|| CliError::Usage("watch requires a source root".to_owned()))?;
    let target = arguments
        .next()
        .ok_or_else(|| CliError::Usage("watch requires a store path".to_owned()))?;
    let mut reconcile = Duration::from_secs(30);
    let mut duration = None;
    let mut poll_only = false;
    let mut index_arguments = Vec::new();
    while let Some(argument) = arguments.next() {
        if argument == "--poll-only" {
            if poll_only {
                return Err(CliError::Usage(
                    "--poll-only must not be repeated".to_owned(),
                ));
            }
            poll_only = true;
        } else if argument == "--reconcile-ms" || argument == "--watch-duration-ms" {
            let value = arguments
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    CliError::Usage(format!("{argument} requires positive milliseconds"))
                })?;
            if argument == "--reconcile-ms" {
                reconcile = Duration::from_millis(value);
            } else {
                duration = Some(Duration::from_millis(value));
            }
        } else {
            index_arguments.push(argument);
        }
    }
    let options = indexing::index_options(index_arguments)?;
    let root = std::fs::canonicalize(root)?;
    if !root.is_dir() {
        return Err(CliError::Usage("watch root must be a directory".to_owned()));
    }
    let target = Path::new(&target);
    let _ownership = ownership::IndexOwnership::acquire(target)?;
    let resolved = if target.exists() {
        std::fs::canonicalize(target)?
    } else {
        let parent = target
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::canonicalize(parent)?.join(
            target
                .file_name()
                .ok_or_else(|| CliError::Usage("store needs a filename".to_owned()))?,
        )
    };
    if resolved.starts_with(&root) {
        return Err(CliError::Usage(
            "watch store must be outside the source root".to_owned(),
        ));
    }
    let stopped = Arc::new(AtomicBool::new(false));
    let signal_stopped = Arc::clone(&stopped);
    ctrlc::try_set_handler(move || signal_stopped.store(true, Ordering::Release))
        .map_err(|error| CliError::Usage(format!("watch signal registration failed: {error}")))?;
    let watch_options = WatchLoopOptions {
        reconcile,
        duration,
        poll_only,
        ..WatchLoopOptions::default()
    };
    run_watch(&root, &watch_options, &stopped, || {
        indexing::index_owned(&root, &resolved, &options, turso)
    })
    .map_err(|error| match error {
        WatchRunError::Pass(cause) => cause,
        WatchRunError::Host(message) => CliError::Usage(message),
    })
}
