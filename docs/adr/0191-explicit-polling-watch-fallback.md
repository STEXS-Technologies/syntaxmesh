# ADR-0191: Explicit polling-only watch fallback

Status: Accepted

## Decision

Add --poll-only to watch/watch-turso. This explicitly bypasses native notification
registration and relies on the existing periodic inventory reconciliation. The
same initial scan, writer lease, Engine recovery, incremental fingerprints,
semantic options and shutdown behavior remain authoritative; there is no second
polling extractor or storage path. --reconcile-ms sets polling frequency, with
the existing 30s default. Quiet/max event debounce is inactive without events.

Native mode remains the default and still fails closed on notification errors;
do not silently downgrade an unhealthy watcher to polling. Operators can restart
with --poll-only where native events are unavailable/unreliable. This trades full
inventory reads and interval-dependent freshness for notification independence.
The existing local database and cross-process Turso restrictions remain unchanged.

## Verification

The real watch subprocess fixture runs native and polling-only modes on File and
verified Turso. Each mode observes initial publication, edit and removal, retains
the initial historical graph, rejects competing writers, and releases the lease
after timed shutdown. Logs require exactly three new generations plus unchanged
reconciliation passes; no-op output cannot satisfy an edit-publication check.
Both watch tests (including the existing signal matrix), strict all-target CLI
Clippy and the workspace architecture gate pass.
