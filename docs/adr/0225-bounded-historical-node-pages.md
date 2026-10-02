# ADR-0225: Bounded historical node pages

- Status: accepted
- Date: 2026-09-30

## Decision

Add `GraphStore::historical_nodes_page(generation, after, limit)` and
`HistoricalNodePage { items, has_more }`, with an exclusive NodeId seek and
limits 1–1000. Return stable-ID-ordered node versions from exactly the requested
generation; determine `has_more` using one lookahead. Unknown generations and
invalid limits fail, including empty/end-of-range requests.

Reuse the existing persistent fact-tree range walker in SQLite and Turso. Share
the adapter-local walk/hydration path with historical name search instead of
copying a second tree reader. Seek into the node-family range, stop at its end,
and validate hydrated identity against its tree key. No delta replay, whole
snapshot reconstruction, new storage format, migration, or transport dependency
is introduced in the durable implementations. Reference backends retain a
snapshot-based default, as their existing historical query defaults do.

This is the reusable storage primitive for bounded graph and diagnostic reads,
not a diagnostic index: filtering all nodes remains proportional to scanned
nodes. Dedicated diagnostic indexing and HTTP/CLI diagnostic attachment remain
open; do not claim this primitive makes diagnostic lookup constant-time.

## Verification

The new conformance fixture compares paged concatenation with exact retained
snapshots across InMemory, File, SQLite, and Turso. It covers sparse/exclusive
seeks, changing page size, empty/end pages, invalid limits, unknown generations,
same-identity updates, removed nodes, empty generations, and durable restart.

Full `cargo make ci` passes in 256.62 seconds, including all sixteen backend
conformance tests, historical search/context regressions, nine attachment tests,
strict workspace Clippy, architecture checks, migration registries, extension
isolation, dependency policy, and API docs. This also verifies the preceding
retained HTTP-client and continuation-identity follow-ups in the full suite.
