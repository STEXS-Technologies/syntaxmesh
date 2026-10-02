# ADR-0084: Expose runtime-neutral historical node point queries

- Status: accepted
- Date: 2026-09-29

## Context

The durable SQLite and Turso stores already implement `GraphStore::historical_node`
using indexed fact-version intervals. The query crate exposes complete historical
`GraphAt` snapshots and node-version histories, but it does not expose that
generation-specific point read. As a result, a runtime-neutral caller that needs
one node from an older generation must either depend directly on a store or
materialize the complete historical graph.

That bypasses the store-port boundary and defeats the layered temporal model for
simple historical inspection. It also makes the efficient backend capability
harder to reuse from the CLI and future hosts.

## Decision

- Expose `Query::historical_node(id)`, scoped to the generation bound to that
  query. It delegates to `GraphStore::historical_node` and returns `Option<Node>`.
- Add explicit `node-at` and `node-at-turso` CLI commands that accept a retained
  generation and stable node ID, then use the query-layer point read.
- Keep `node(id)` as a current-projection read and `graph_at(generation)` as a
  complete output-sized read. Do not silently change either operation's contract.
- Durable SQLite/Turso point reads must remain bounded by generation lookup,
  indexed identity/validity lookup, and one returned payload; their cost must not
  grow with the number of intervening generations. The in-memory/file reference
  implementation may use the existing replay fallback.
- Do not add a DTO or SQL-specific detail. The query API returns the canonical
  `Node`; host-specific rendering remains in the CLI/protocol layer.

## Consequences

- Embedded callers can inspect historic state without materializing the entire
  graph or depending on a durable backend type.
- The generation-scoped API clearly distinguishes point reads from complete
  graph exports and from longitudinal `History(node)`.
- Differential tests must confirm old/current/deleted node behavior across
  reference, SQLite, and Turso stores. Existing durable store tests remain the
  evidence for indexed rather than replay-based point reads.
- This is a storage/query foundation, not calendar-valid-time selection or a
  claim of strict O(1) complexity: durable indexes are logarithmic and return
  work is proportional to the requested payload.
