use std::time::Duration;

use super::WatchCoalescer;

fn millis(value: u64) -> Duration {
    Duration::from_millis(value)
}

fn check(condition: bool) -> Result<(), &'static str> {
    if condition {
        Ok(())
    } else {
        Err("watch scheduling invariant failed")
    }
}

#[test]
fn invalid_intervals_are_rejected() {
    assert!(WatchCoalescer::new(Duration::ZERO, millis(10)).is_err());
    assert!(WatchCoalescer::new(millis(10), millis(9)).is_err());
}

#[test]
fn remaining_time_tracks_quiet_and_maximum_without_overflow() -> Result<(), &'static str> {
    let mut policy = WatchCoalescer::new(millis(10), millis(30))?;
    check(policy.time_until_due(millis(0)).is_none())?;
    policy.mark(millis(0));
    check(policy.time_until_due(millis(8)) == Some(millis(2)))?;
    policy.mark(millis(29));
    check(policy.time_until_due(millis(29)) == Some(millis(1)))?;
    check(policy.time_until_due(millis(30)) == Some(Duration::ZERO))?;
    check(policy.take_due(millis(30)))?;
    policy.mark(Duration::MAX);
    check(policy.time_until_due(Duration::MAX) == Some(millis(10)))?;
    Ok(())
}

#[test]
fn quiet_window_merges_events_and_consumes_once() -> Result<(), &'static str> {
    let mut policy = WatchCoalescer::new(millis(10), millis(100))?;
    check(!policy.take_due(millis(500)))?;
    policy.mark(millis(1));
    policy.mark(millis(8));
    check(!policy.take_due(millis(17)))?;
    check(policy.is_due(millis(18)))?;
    check(policy.take_due(millis(18)))?;
    check(!policy.take_due(millis(500)))?;
    Ok(())
}

#[test]
fn continuous_events_cannot_postpone_maximum() -> Result<(), &'static str> {
    let mut policy = WatchCoalescer::new(millis(10), millis(30))?;
    for time in 0..30 {
        policy.mark(millis(time));
    }
    check(!policy.take_due(millis(29)))?;
    check(policy.take_due(millis(30)))?;
    Ok(())
}

#[test]
fn events_during_indexing_and_failed_work_form_new_batches() -> Result<(), &'static str> {
    let mut policy = WatchCoalescer::new(millis(10), millis(30))?;
    policy.mark(millis(0));
    check(policy.take_due(millis(10)))?;
    policy.mark(millis(11));
    check(!policy.take_due(millis(20)))?;
    check(policy.take_due(millis(21)))?;
    policy.mark(millis(22)); // Host retries a failed indexing operation.
    check(policy.take_due(millis(32)))?;
    Ok(())
}

#[test]
fn out_of_order_time_and_extreme_durations_do_not_overflow() -> Result<(), &'static str> {
    let mut policy = WatchCoalescer::new(millis(10), millis(30))?;
    policy.mark(millis(10));
    policy.mark(millis(5));
    check(!policy.take_due(millis(0)))?;
    check(!policy.take_due(millis(19)))?;
    check(policy.take_due(millis(20)))?;
    policy.mark(Duration::MAX);
    check(!policy.take_due(Duration::MAX))?;
    Ok(())
}
