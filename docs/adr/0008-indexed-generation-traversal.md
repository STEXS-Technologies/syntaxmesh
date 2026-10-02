# ADR 0008: Traverse through generation-scoped adjacency indexes

- Status: accepted
- Date: 2026-09-27

## Context

`Query` currently constructs a complete `GenerationGraph` for each neighbors, path, impact, and subgraph request. The canonical in-memory store already owns all nodes and edges, but outgoing-edge reads scan the full edge map; incoming reads do not exist on the store port. This repeats work and makes local query cost proportional to the full graph before traversal begins.

## Decision

Add a default `GraphStore::incoming` read operation that filters the existing edge method, preserving compatibility for custom backends. `InMemoryGraphStore` will maintain ephemeral incoming/outgoing adjacency and file-owner indexes, rebuilding them after snapshot restore and graph delta application. File and Turso stores delegate to those indexes. Query traversal will use generation-checked `node`, `outgoing`, and `incoming` reads directly and will no longer materialize `GenerationGraph` per traversal.

Adjacency indexes are derived caches, not canonical state. File snapshot serialization skips them; opening a file snapshot rebuilds them from canonical nodes and edges. Turso restart already rebuilds canonical state, which also rebuilds the indexes. The standalone `GenerationGraph` projection remains available for callers that explicitly need a complete projection.

## Consequences

- Neighbors/path/impact/subgraph traversal visits only reached adjacency lists and no longer allocates a full graph projection for each request.
- Applying a delta or opening a file snapshot rebuilds derived indexes in O(V+E); write/read startup costs increase by that rebuild.
- Search substring, full-generation export, and status counts still inspect generation-wide state. Turso startup now validates indexed projections and streams the root without restoring a whole graph into memory; see [ADR-0024](0024-turso-row-authoritative-storage.md).

## Verification

Compare indexed query results to generation projection behavior; test incoming/outgoing indexes after file restart and Turso restart; run full backend conformance and `cargo make ci`.
