# ADR-0156: Generation-root temporal substring search

- Status: accepted
- Date: 2026-09-30

## Decision

Add historical_search_nodes to GraphStore and historical_search to pinned Query.
Reuse PersistentFactTreeRangeWalker over the selected immutable generation root,
starting immediately before the node family and stopping at the next family.
SQLite and Turso decode node facts only, return stable-ID-ordered matches up to
the requested limit, and discard the loaded-page cache after each yielded node.
The walker retains traversal progress; cache disposal does not restart scanning.
Validate the generation even for zero limits and empty results.

Preserve the current case-insensitive substring contract, including empty text.
The reference default filters its historical snapshot. Durable implementations
must not replay deltas or materialize a snapshot. No new schema or projection is
needed: use the existing root, page codec, content-address checks, and migration
backfills. Missing/corrupt pages fail instead of falling back to current state.

This scans nodes in one generation. It is not constant-time, indexed lexical
matching, or proof of search scalability. It avoids scanning every historical
version, and provides a correct temporal primitive needed by ADR-0154; ordinary
context routing and the MCP generation argument remain separate work.

## Verification

Cross-backend fixtures must cover replacement/deletion, substring/case matching,
empty queries, bounded stable ordering, unknown generations, and restart.

Those differential fixtures pass on InMemory, File, SQLite, and Turso. Durable
fixtures also verify the same search across 70 generations/checkpoint depth.
Full `cargo make ci` passes. No latency or constant-time claim is established.
