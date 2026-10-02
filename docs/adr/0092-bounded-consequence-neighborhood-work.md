# ADR-0092: Bound consequence-neighborhood traversal work

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0044 added deterministic consequence-graph BFS with endpoint and returned
edge limits. Unlike `Query::historical_neighborhood`, however, it has no hard
ceiling for those caller limits and no bound on incident rows examined. One
query can therefore request huge allocations or scan a high-degree endpoint
for a long time. The source-of-truth requires explicit bounds on interactive
historical traversals, and the existing historical-neighborhood implementation
already provides a tested pattern for hop, item, and incidence-scan budgets.

## Decision

1. Apply the existing bounded-BFS semantics to consequence traversal; do not
   add a second traversal algorithm. Keep weak connectivity, deterministic
   ordering, edge direction, shortest discovered hop, and pinned-generation
   behavior unchanged.
2. Require a positive maximum scanned-incidence count. Count every returned
   endpoint-edge entry examined, including the same edge encountered from its
   opposite endpoint. Enforce hard maxima of 64 hops, 10,000 endpoints, 10,000
   output edges, and 100,000 scanned incidences, matching the established
   historical-neighborhood ceilings.
3. Page through consequence endpoint results in bounded batches so the scan
   budget, rather than an arbitrarily large SQL result allocation, controls
   work. Reuse the existing endpoint cursor and store index.
4. Return the scanned-incidence count with the query result and NDJSON footer.
   Upgrade the temporal-record schema version because the footer shape changes.
   A scan, endpoint, or edge cap that omits eligible work sets `truncated`;
   reaching the hop bound alone remains intentional and does not.
5. Do not add inference, trust ordering, generation-range propagation, or
   ChangeSet authoring here. Those need separate semantic decisions and remain
   outside this bounded fixed-generation query.

## Consequences

- Callers must supply an additional scan budget; oversized or zero bounds fail
  before storage access with `InvalidLimit`.
- High-degree traversals can report truncation even when output edge capacity
  remains, making the work limit observable instead of silently expensive.
- The result remains a fixed-generation neighborhood, not the separate
  multi-generation `ConsequenceTrace` described in the broader roadmap.

## Verification

Cover exact count and output at every cap, duplicate-edge scan accounting,
high-degree pagination, deterministic continuation across pages, cursor and
generation binding, and File/SQLite/Turso equivalence. Run the full CI and
architecture gates; no schema migration is required because only query and
interchange contracts change.
