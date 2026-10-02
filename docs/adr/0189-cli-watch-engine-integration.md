# ADR-0189: CLI watch through existing indexing

Status: Accepted

## Decision

Add watch/watch-turso commands with explicit source and store paths and the
existing index options. Acquire the existing writer lease for the entire host
lifetime. Register native notifications before initial indexing; subsequent
rescans call the same index implementation without recursively acquiring a lease.
Store files must reside outside the watched source root to avoid generated-store
feedback and accidentally indexing generated outputs. Turso still needs explicit
migration before watch; no automatic migration is introduced.

Coalesce notifications with 100ms quiet and 1s maximum delay. Reconcile inventory
every 30s even without notifications; --reconcile-ms permits a positive interval
override. --watch-duration-ms optionally stops after a positive elapsed duration.
Continuous events cannot postpone reconciliation. Completed passes retain the
existing source fingerprint no-op behavior, semantic cache, Penelope recovery and
opt-in StateChronicle verification. No NDJSON is used between components.

Use ctrlc 3.5.2's termination handler to request shutdown on Ctrl-C and Unix
SIGTERM/SIGHUP. Finish an admitted index operation before release; do not interrupt
database work. This has no bounded shutdown deadline. Backend degradation,
notification disconnection, or indexing errors fail the command; restart uses
the existing durable recovery path rather than inventing another retry workflow.

This is a local single-writer watch host, not syntaxmeshd: daemon IPC, projection
refresh, concurrent generation switching in query hosts, analytics scheduling,
and full daemon health/lifecycle integration remain open.

Turso's current adapter can reject database opens while a separate reader process
holds its backend lock. Do not run separate Turso query hosts/CLI readers against
an active watch writer; same-process query serving remains required future work.
The File host permits read commands while watching. This is not daemon-equivalent
concurrent serving, and exhaustive startup recovery of pending non-index work
is not established by the existing indexing path's no-op optimization.
ADR-0190 supersedes that publication-recovery gap: reconciliation now precedes
the no-op decision. Semantic-provider and other service recovery remain separate.

## Verification

A subprocess test covers initial indexing, edits, removals, historical facts,
competing-writer rejection, duration shutdown and lease release for File and
verified Turso. It queries File while watching and both stores after shutdown;
it does not claim concurrent cross-process Turso reads. Strict CLI Clippy passes.
Full `cargo make ci` passed in 436.68 seconds, including dependency rebuilds,
workspace tests and documentation. Its audit registry yank lookup timed out;
a separate `cargo audit` retry completed successfully with only the two existing
allowed unmaintained-dependency warnings.

Unix subprocess coverage now sends real SIGTERM, SIGHUP and SIGKILL to idle
File/verified-Turso watch owners after initial indexing. Graceful signals return
success; forced death does not. Subsequent indexing reacquires ownership, retained
facts remain queryable and StateChronicle history verifies. Tests reuse nix
0.31.3, already supplied by ctrlc, as a Unix-only development dependency; they
launch no shell signal utility and introduce no production signal implementation.
This is idle-process lifecycle coverage, not kill-during-commit or power-loss
recovery evidence. Both watch tests, strict CLI Clippy and architecture checks pass.
Full `cargo make ci` for the recovery gate and signal fixtures passed in 151.41 seconds.
