# ADR 0009: Indexed Turso Read Path

- Status: accepted
- Date: 2026-09-27
- Decision owners: SyntaxMesh maintainers

## Context

Turso currently persists graph facts as payload rows, but restores every row into an `InMemoryGraphStore` at open and serves reads from that complete projection. The v0 architecture requires Turso as the canonical embedded store and query paths that do not load the complete graph for ordinary lookups. The current representation also has no SQL indexes for endpoint, owner-file, or name queries.

## Decision

Add indexed scalar columns for node name and owner file, and edge source and target, retaining the versioned serialized payload as the canonical typed record. Migrate the current schema transactionally and reject unsupported schema versions. Implement default `GraphStore` lookup methods for compatibility; Turso overrides node, exact-name/terminal-name symbol candidates, outgoing/incoming, and nodes-for-file with indexed SQL queries bound to the requested generation. File and in-memory stores retain deterministic implementations. Substring `search_nodes` remains a SQL `instr(lower(name), ...)` scan: the name B-tree index does not accelerate this operation.

The full-state restore remains in place for write validation and graph-root verification during this step. Replacing full-state write staging and startup validation is a separate decision requiring equivalent atomicity and integrity evidence.

## Consequences

- Ordinary Turso node/traversal/exact-symbol/file-owner reads no longer scan the full graph or depend on the in-memory graph projection. Substring search still scans names in SQL.
- Payloads remain the lossless encoding; indexed columns must be checked against payloads during restart validation and updated in the same transaction as payload writes.
- One extra schema migration and index-maintenance cost are introduced.
- Full graph restoration and full-delta snapshot replacement remain known scaling limits.

This implementation limitation was subsequently removed by
[ADR-0024](0024-turso-row-authoritative-storage.md): current Turso rows are
authoritative, startup root validation streams facts, and delta publication
touches only changed rows.

## Verification

- Test migration from the preceding schema and rejection of unknown versions.
- Test SQL-backed lookups against the reference store for node, search, both edge directions, and file ownership.
- Reopen the database and repeat lookups; run the complete backend conformance and workspace CI suites.
