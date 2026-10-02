# ADR-0188: Bounded filesystem notification adapter

Status: Accepted

## Decision

Use crates.io notify 8.2.0's recommended recursive watcher in a separate
syntaxmesh-watch-host adapter. Keep syntaxmesh-watch's scheduling policy free
of OS notification dependencies. Retain the watcher for the adapter lifetime.

The callback performs no indexing and uses a capacity-one nonblocking channel.
Queued notifications represent inventory invalidation, not exact path deltas.
Queue saturation therefore merges work rather than growing memory. Watcher
errors set a sticky degraded flag and enqueue a rescan; consumers must treat
degraded delivery as requiring reconciliation/restart, not healthy monitoring.
Ignore access-only events to avoid scan/read feedback, except explicit rescan
flags. Caller-provided absolute excluded paths suppress self-generated store
and output changes; event errors/rescan flags bypass path filtering.

The host must register before its initial scan, retain events during indexing,
apply the existing coalescer, and invoke existing Engine workflows under its
ownership lease. This adapter does not claim notification completeness, runtime
recovery, CLI wiring, periodic reconciliation, or a working daemon. Upstream
documents filesystem/backend limits: https://docs.rs/notify/8.2.0/notify/.

## Verification

Tests verify 10,000 callbacks retain only one invalidation, an error while the
queue is full remains sticky, excluded/access events are suppressed, explicit
rescan flags bypass exclusions, and the native watcher reports a real file write.
All three tests, strict all-target package Clippy, workspace architecture checks,
and cargo-deny pass. Cargo-deny's existing cfg_block license-field warning remains.
Native filesystem delivery is tested on this Linux workspace, not all platforms.
