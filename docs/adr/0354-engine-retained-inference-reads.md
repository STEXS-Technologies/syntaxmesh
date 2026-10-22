# ADR-0354: Engine retained inference reads

Status: opt-in checkpoint and page service implemented; host delivery pending.

## Decision

Expose opt-in engine checkpoint and bounded page reads over the existing downstream
materializer. Check the supplied origin's repository/worktree against engine scope
before opening the destination. The materializer validates the complete retained
origin, checkpoint, cursor, root and selected payloads within its read transaction.
Do not load canonical history or replay rules: downstream retained selections are
the read authority, including after canonical storage is unavailable.

Opening remains non-migrating. Reads neither recover workflows nor publish effects.
Reuse existing adapter request/results in this host-facing opt-in engine service;
do not add database dependencies to pure public transport DTOs or protocol crates.
Host transport contracts and commands require separate integration.

## Required evidence

File/Turso engine fixtures compare retained baseline and retracted pages with
direct materializer reads after reopening. Foreign origins reject before opening
a missing database. Zero admission and substituted cursors reject. Existing
workflow recovery and canonical verification tests remain green.

## Implemented evidence

Nineteen feature-enabled engine tests pass. File/Turso fixtures compare exact
retained pages with direct adapter reads after reopening, read empty retracted
selections, reject zero budgets and substituted roots, and reject foreign scope
before opening a missing destination. An engine with an empty canonical store
returns the same retained page and checkpoint, proving these reads do not require
canonical history. Existing recovery and opt-in verification tests remain green.
Strict all-target Clippy and the architecture gate pass. This is targeted evidence,
not a subsequent full contributor CI run or completed host transport delivery.

Subsequent full contributor CI passed on 2026-10-09 in 293.87 seconds, including
all-feature workspace tests, strict Clippy and API documentation. Host delivery
and higher-layer product requirements remain unfinished.
