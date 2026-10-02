# ADR-0231: Bounded historical file pages

- Status: accepted
- Date: 2026-09-30

## Decision

Add `HistoricalFilePage` and `GraphStore::historical_files_page` using the same
exclusive stable-ID seek and one-lookahead semantics as historical node pages.
Limits are 1–1000. The reference implementation sorts retained snapshot files;
SQLite and Turso must use the existing generation fact-tree file family instead
of delta replay, whole-snapshot hydration, or a new SQL schema.

File facts are family zero, so an initial walk has no lower-bound key. Do not
underflow the family identifier when adapting the node-family reader. Validate
hydrated FileVersion IDs against their tree keys and fail on missing/corrupt
pages. Retained generations and restart must preserve exact inventory versions.

This is a store primitive for later paged status inventory transport; it does
not itself remove the current HTTP status response limit. Public core/runtime
contracts remain host-independent, and no NDJSON handoff is added.

## Verification gates

Cross-backend comparison must cover stable ordering, exclusive seeks, lookahead,
changed/removed files, empty generations, invalid limits, unknown generations,
and durable restart. Native adapter implementations are required before claiming
bounded durable reads.

Native SQLite/Turso file readers reuse retained roots, the range walker,
canonical fact decoding, and existing persistent-tree error mapping. The extended
four-backend node/file paging fixture passes with 64 additional file versions,
changed and removed inventory entries, empty inventory, sparse seeks, lookahead,
invalid limits, unknown generations, and File/SQLite/Turso restart. Strict
affected all-feature Clippy passes. Paged HTTP/CLI status inventory is implemented
and verified under ADR-0232; full `cargo make ci` passes in 236.37 seconds for
both slices. Per-response limits and total CLI inventory memory/time remain.
