# ADR-0202: Reusable callback watch loop

## Decision

Move the existing CLI watch loop into `syntaxmesh-watch-host::run_watch`.
Callers supply a fallible synchronous callback, stop flag, and explicit timing
options. Reuse native notification registration, bounded coalescing, and shared
deadline selection. Register notifications before the initial callback. Periodic
reconciliation remains mandatory; polling-only mode is explicit, not automatic.

The CLI supplies its unchanged indexing callback and retains source/store
validation, writer ownership, signal registration, and provider policy. The
watch host owns no Engine, store, workflows, or transport. Callback failures
and degraded/disconnected notifications fail closed. Stop waits for an admitted
callback to finish; no cancellation or bounded active-pass deadline is claimed.

This is the same host loop usable by a future daemon around its shared Engine,
not a separate indexing implementation. Daemon IPC and scheduling remain open.

## Verification

Retain CLI native/polling edit/removal/history/no-op and signal-death fixtures;
add host callback-error and stop/invalid-configuration fixtures.

Verified: five host-crate tests and both CLI native/polling/signal watch tests
pass. Strict host/CLI Clippy, formatting, and architecture checks pass. Full
workspace CI was not rerun for this extraction.

A follow-up real TCP composition fixture joins `run_watch` to the shared HTTP
Engine without opening a second Turso handle. Native and polling saves publish
verified updates, retract the old current symbol, retain explicit historical
search, and audit StateChronicle after worker shutdown. All eight HTTP tests,
strict HTTP Clippy, formatting, and architecture checks pass. This is controlled
integration evidence, not a production daemon command or general no-op policy:
the fixture callback handles one known edit with fixture-local source comparison.

Follow-up full `cargo make ci` passed in 185.88 seconds, covering the shared
HTTP host, generic/Send extractor registry, reusable watch loop, composition
fixtures, all-feature workspace tests, and documentation. Existing allowed
dependency warnings remain for `bincode`, `paste`, and `cfg_block`.
