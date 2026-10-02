use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use syntaxmesh_watch::{WatchCoalescer, next_watch_wait};

use crate::FilesystemNotifications;

mod options;
#[cfg(test)]
mod tests;

pub use options::WatchLoopOptions;

/// A host notification/configuration failure or caller callback failure.
#[derive(Debug)]
pub enum WatchRunError<E> {
    Host(String),
    Pass(E),
}

impl<E: std::fmt::Display> std::fmt::Display for WatchRunError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(message) => formatter.write_str(message),
            Self::Pass(error) => error.fmt(formatter),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for WatchRunError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pass(error) => Some(error),
            Self::Host(_) => None,
        }
    }
}

/// Run the shared notification/reconciliation loop around a caller's index pass.
/// Native registration precedes the initial pass. Stop is checked between passes,
/// not within an admitted callback. The caller owns store locking and recovery.
///
/// # Errors
/// Returns invalid timing, notification failure, or the first callback error.
pub fn run_watch<E>(
    root: &Path,
    options: &WatchLoopOptions,
    stopped: &AtomicBool,
    mut pass: impl FnMut() -> Result<(), E>,
) -> Result<(), WatchRunError<E>> {
    if options.reconcile.is_zero()
        || options.maximum_wait.is_zero()
        || options.duration.is_some_and(|limit| limit.is_zero())
    {
        return Err(WatchRunError::Host(
            "watch intervals must be positive".to_owned(),
        ));
    }
    let mut policy = WatchCoalescer::new(options.quiet, options.maximum)
        .map_err(|error| WatchRunError::Host(error.to_owned()))?;
    let notifications = if options.poll_only {
        None
    } else {
        Some(
            FilesystemNotifications::open(root, vec![]).map_err(|error| {
                WatchRunError::Host(format!("watch registration failed: {error}"))
            })?,
        )
    };
    let started = Instant::now();
    pass().map_err(WatchRunError::Pass)?;
    let mut last_scan = started.elapsed();
    while !stopped.load(Ordering::Acquire)
        && !options
            .duration
            .is_some_and(|limit| started.elapsed() >= limit)
    {
        let wait = next_watch_wait(
            &policy,
            started.elapsed(),
            last_scan,
            options.reconcile,
            options.duration,
            options.maximum_wait,
        );
        let notice = if let Some(notifications) = &notifications {
            notifications.receive(wait).map_err(|error| {
                WatchRunError::Host(format!("watch notifications disconnected: {error}"))
            })?
        } else {
            std::thread::sleep(wait);
            None
        };
        if let Some(notice) = notice {
            if notice.degraded {
                return Err(WatchRunError::Host(
                    "watch backend degraded; restart to reconcile".to_owned(),
                ));
            }
            policy.mark(started.elapsed());
        }
        if stopped.load(Ordering::Acquire)
            || options
                .duration
                .is_some_and(|limit| started.elapsed() >= limit)
        {
            break;
        }
        let now = started.elapsed();
        if policy.take_due(now) || now.saturating_sub(last_scan) >= options.reconcile {
            pass().map_err(WatchRunError::Pass)?;
            last_scan = started.elapsed();
        }
    }
    Ok(())
}
