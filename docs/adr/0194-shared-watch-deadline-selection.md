# ADR-0194: Shared watch deadline selection

## Decision

Move the tested CLI deadline selector into `syntaxmesh-watch` as
`next_watch_wait`. Accept a caller-supplied maximum wait instead of embedding
the CLI's 50 ms signal-check policy. This reuses the existing scheduler rather
than introducing separate daemon timing semantics.

The pure function takes monotonic elapsed durations, the last completed scan,
reconciliation interval, and optional stop deadline. It returns the earliest
remaining debounce, reconciliation, stop, or caller maximum interval. Saturating
arithmetic preserves expired deadlines and avoids duration overflow.

CLI watch retains its 50 ms maximum wait. Other hosts select their own maximum;
this does not add runtime dependencies, sleep, cancellation, IPC, or a daemon.

## Verification

Move the existing deterministic deadline fixture and add caller-cap coverage.
Retain real native/polling watch and shutdown integration coverage.

Verified: eight watch-crate unit tests and both CLI native/polling/signal watch
tests pass. Targeted strict Clippy, formatting, and architecture checks pass.
Full workspace CI was last run before this extraction, not after it.
