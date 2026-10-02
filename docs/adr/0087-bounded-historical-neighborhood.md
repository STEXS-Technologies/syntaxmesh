# ADR-0087: Bounded generation-pinned historical neighborhoods

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0086 exposes one bounded adjacency page at an exact retained generation.
That makes point-in-time inspection efficient but does not yet support a
multi-hop historical neighborhood. The product source of truth requires
bounded temporal traversals; it also forbids an interactive query from
silently scanning unlimited history. The existing consequence neighborhood
provides deterministic weak-connectivity BFS semantics, and Shardline's
bounded preview/query patterns provide the right operational principle:
separately bound traversal work and returned data.

This first slice traverses one exact generation only. It is not a
time-respecting traversal across generations, calendar-valid-time query, or
transitive causal inference. Relation/evidence/repository filters and
generation-range or calendar-range selection remain separate source-roadmap
requirements and are not implied by this API.

## Decision

1. Add a runtime-neutral query operation over the `Query`'s exact pinned
   generation. It uses deterministic weakly-connected BFS, sorted unique
   seeds/frontiers, stable edge-ID order, and preserves each edge's direction.
2. Require positive maximum hops, selected nodes, returned edges, and scanned
   incidence entries. Enforce absolute caps for each. A page is requested
   from the generation-scoped incidence index with at most the remaining
   scan budget plus one lookahead entry; no generation deltas or complete
   graph are replayed/materialized by the traversal.
3. Report `truncated` when a scan, node, or edge cap omits eligible work or
   output. Reaching the requested hop depth alone is intentional and is not
   truncation. Seeds must be present in the pinned generation and fit the
   node cap.
4. Return selected node identities and each selected edge with its shortest
   discovered BFS depth. The query does not infer causality or reinterpret
   edge direction. Consumers can hydrate node details through generation-
   pinned point queries.
5. Keep the traversal semantics in the public Rust query API. Serialization
   and CLI presentation are a separate contract, subsequently specified and
   implemented by [ADR-0088](0088-historical-neighborhood-stream-and-cli.md).

## Consequences

- Durable query work is bounded by visited incidence entries plus index seeks,
  independent of the number of intervening generations; output cost remains
  proportional to returned nodes/edges. The explicit scan cap prevents
  high-degree or cyclic neighborhoods from causing unbounded work.
- InMemory/File remain reference implementations and may reconstruct the
  requested generation; they must still honor the same scan and output caps.
- The operation composes the existing consequence-BFS semantics and
  historical-neighbor page API rather than introducing another traversal or
  adjacency representation.
- [ADR-0088](0088-historical-neighborhood-stream-and-cli.md) adds a
  versioned NDJSON representation and paired File/Turso CLI commands over this
  same query operation.
- Generation-range/calendar selection, relation and evidence-class filters,
  repository/concept scopes, a resumable whole-neighborhood cursor, and broader
  analytical propagation remain open follow-up work.

## Verification

Test deterministic weak BFS, edge direction, cycles, duplicate seeds,
disconnected seeds, missing seeds, all four bounds, truncation versus an
intentional hop limit, and exact historical retraction/reintroduction behavior
across reference and durable stores. Include a fixed-output history-depth
fixture; do not claim strict O(1) traversal or a latency SLA.
