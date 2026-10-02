# ADR-0192: Deadline-aware watch waiting

Status: Accepted

## Decision

Expose WatchCoalescer::time_until_due using host-supplied monotonic elapsed time.
Return None without pending work and zero when pending work is due; compute the
minimum remaining quiet/max interval using saturating subtraction, not deadline
addition that could overflow. is_due delegates to that policy result.

The CLI wait is the minimum of pending debounce, periodic reconciliation, optional
timed shutdown, and the existing 50ms signal-check ceiling. Native receive and
poll-only sleep use that same duration. This avoids adding a fixed 50ms overshoot
to shorter requested intervals while keeping signal checks bounded between index
passes. It is not a real-time execution guarantee or cancellation of active work.
No new timer runtime, notification backend, workflow or storage path is added.

## Verification

Six policy tests pass, including remaining quiet/max time, due consumption and
extreme durations. A deterministic host-wait test covers shorter reconciliation,
shutdown, signal-check and event deadlines. Both real watch fixtures pass across
native/polling File/verified-Turso and Unix termination/death cases. Strict
all-target Watch/CLI Clippy, formatting and architecture checks pass.
