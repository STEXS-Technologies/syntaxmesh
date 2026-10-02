# ADR-0043: Indexed, snapshot-bound consequence endpoint reads

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0042 defines evidence-backed consequence edges and requires bounded indexed
traversal. The canonical stores now publish those edges atomically and maintain
source/target temporal indexes, but callers cannot read them. A first useful
primitive is a direct incident-edge page; bounded multi-hop traversal can be
composed over that primitive without replaying accepted deltas.

## Decision

1. `consequence_edges_for_endpoint(endpoint, as_of, after, limit)` returns edges
   whose exact source or target equals the supplied typed endpoint and which are
   active at the selected retained generation. Exact `FactVersionRef` generation
   remains part of endpoint equality.
2. Results are ordered by stable `ConsequenceEdgeId`, limited to `limit`, and
   use a `limit + 1` probe to determine whether another page exists. The cursor
   carries endpoint, snapshot generation, and last edge ID; using it with another
   endpoint or snapshot is invalid.
3. SQLite and Turso answer from the source/target interval indexes in the same
   query snapshot. InMemory and File may derive the active set from their
   retained consequence journal as conformance/reference implementations.
4. This is a direct adjacency page, not an unbounded traversal. Any multi-hop
   query must separately enforce explicit depth and total-result limits and
   preserve typed consequence/evidence/provenance data.

## Consequences

- Add a typed core cursor, store page/port method, durable indexed readers, and
  query-service projection.
- No causality is inferred; reads return only explicitly published edges.
- This endpoint primitive does not itself provide historical state-at-time
  semantics or transitive propagation.
