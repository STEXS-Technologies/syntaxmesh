use std::time::Duration;

use crate::WatchCoalescer;

#[cfg(test)]
mod tests;

/// Select the earliest remaining watch deadline using monotonic elapsed times.
/// `last_scan` is the elapsed time at the end of the last completed scan.
/// Zero is returned for expired deadlines or a zero caller maximum wait.
/// This function neither sleeps nor performs indexing.
#[must_use]
pub fn next_watch_wait(
    policy: &WatchCoalescer,
    now: Duration,
    last_scan: Duration,
    reconcile: Duration,
    limit: Option<Duration>,
    maximum_wait: Duration,
) -> Duration {
    let mut wait = maximum_wait.min(reconcile.saturating_sub(now.saturating_sub(last_scan)));
    if let Some(remaining) = policy.time_until_due(now) {
        wait = wait.min(remaining);
    }
    if let Some(limit) = limit {
        wait = wait.min(limit.saturating_sub(now));
    }
    wait
}
