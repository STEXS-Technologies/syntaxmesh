use super::*;

#[test]
fn caller_controls_maximum_wait_without_clock_overflow() -> Result<(), &'static str> {
    let policy = WatchCoalescer::new(Duration::from_millis(10), Duration::from_millis(30))?;
    for cap in [Duration::ZERO, Duration::from_millis(7), Duration::MAX] {
        if next_watch_wait(
            &policy,
            Duration::MAX,
            Duration::MAX,
            Duration::MAX,
            None,
            cap,
        ) != cap
        {
            return Err("caller maximum wait was changed or overflowed");
        }
    }
    Ok(())
}

#[test]
fn wait_respects_every_deadline_without_fixed_tick_overshoot() -> Result<(), &'static str> {
    let mut policy = WatchCoalescer::new(Duration::from_millis(10), Duration::from_millis(30))?;
    for (now, reconcile, limit, expected) in [
        (0, 100, None, 50),
        (0, 5, None, 5),
        (5, 5, None, 0),
        (0, 100, Some(3), 3),
        (3, 100, Some(3), 0),
    ] {
        if next_watch_wait(
            &policy,
            Duration::from_millis(now),
            Duration::ZERO,
            Duration::from_millis(reconcile),
            limit.map(Duration::from_millis),
            Duration::from_millis(50),
        ) != Duration::from_millis(expected)
        {
            return Err("host wait missed reconciliation/shutdown/signal deadline");
        }
    }
    policy.mark(Duration::ZERO);
    if next_watch_wait(
        &policy,
        Duration::from_millis(8),
        Duration::ZERO,
        Duration::from_secs(1),
        None,
        Duration::from_millis(50),
    ) != Duration::from_millis(2)
    {
        return Err("host wait missed debounce deadline");
    }
    policy.mark(Duration::from_millis(29));
    if next_watch_wait(
        &policy,
        Duration::from_millis(29),
        Duration::ZERO,
        Duration::from_secs(1),
        None,
        Duration::from_millis(50),
    ) != Duration::from_millis(1)
    {
        return Err("new events postponed maximum deadline");
    }
    Ok(())
}
