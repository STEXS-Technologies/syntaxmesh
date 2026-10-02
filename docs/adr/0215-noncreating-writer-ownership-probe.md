# ADR-0215: Non-creating writer ownership probe

- Status: accepted
- Date: 2026-09-30

## Decision

Add host-only `is_writer_active` for an existing regular store file. Resolve the
canonical target and use the same persistent sidecar naming and standard-library
exclusive lock as `WriterLease`. Open only an existing regular sidecar without
creation or truncation. A missing sidecar means no observed cooperating writer;
a contended lock means an active writer. Other filesystem errors fail closed.
Reject symlink and non-regular sidecars. Release a successful probe lock on return.

This reuses the existing lease implementation rather than PID liveness or a new
locking dependency. No database is opened and no file contents are modified.
The result is a transient observation, not authorization to open a database or
mutate it: a writer can start immediately after the probe. Direct-operation
fallback must still acquire and retain the real lease across store use. Active
owners will require separately validated endpoint discovery and session identity;
this probe alone does not implement daemon attachment or authenticate an owner.

Like the existing lease, path checks assume operator-controlled directories and
do not provide race-free hostile-directory anchoring. No pure crate gains host
dependencies and no graph/wire representation changes.

## Verification

Ten ownership-host tests pass, covering non-creation, preserved contents,
active ownership, probe-lock release, regular-file validation, and aliases.
Eight daemon process tests pass; native and polling fixtures observe ownership
held by a separate process and released after abrupt death and graceful exit.
Strict ownership/daemon Clippy, workspace formatting, and architecture checks
pass. Full `cargo make ci` passes with this probe (155.55 seconds), including
workspace all-feature tests, dependency policy, migration registries, extension
isolation, and API documentation. Existing allowed dependency warnings remain.
