# ADR-0044: Bounded consequence neighborhoods

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0036 requires bounded lineage traversal whose cost depends on indexed
lookups and returned nodes/edges, not total retained generation depth. ADR-0043
provides a fixed-generation, cursor-paged direct endpoint lookup. SyntaxMesh
already has a deterministic bounded weakly connected BFS contract for ordinary
graph subgraph export in ADR-0007; consequence history should reuse those
semantics rather than introduce a second traversal model.

## Decision

1. `Query::consequence_neighborhood` accepts non-empty exact endpoint seeds,
   maximum hops, maximum unique endpoints, and maximum unique consequence
   edges. It is pinned to the `Query` generation and traverses incident edges
   weakly connected (both source-to-target and target-to-source); the original
   edge direction remains present in every returned edge.
2. Use deterministic breadth-first order: sorted unique seeds, then sorted
   endpoints per frontier, with each indexed adjacency page already ordered by
   stable edge ID. Return reached endpoints and each edge with its shortest
   discovered hop depth. Duplicate seeds and edges do not duplicate output.
3. Seeds must be non-empty and fit the endpoint cap; all limits must be
   positive. A new edge is omitted if either the edge cap is reached or it
   would introduce an endpoint beyond the endpoint cap. Either omission sets
   `truncated`. Reaching the hop bound alone is intentional and does not set
   `truncated`, matching ADR-0007.
4. Read one endpoint at a time using ADR-0043's index-backed direct adjacency
   page. No historical delta replay, path-based inference, or causality
   inference is permitted. This is bounded context retrieval, not general
   transitive impact analysis.

## Consequences

- The query crate returns a typed neighborhood result with explicit hop depth
  and truncation status.
- Multi-hop output remains distinct from non-causal ChangeEventCorrelation;
  every edge retains its consequence kind, evidence, derivation, and producer.
- Versioned NDJSON and CLI integration are defined separately in
  [ADR-0045](0045-consequence-neighborhood-ndjson.md). Resumable
  whole-neighborhood cursors, bitemporal state selection, and broad analytical
  propagation remain separate follow-up work.

## Verification

Test direction-agnostic reachability, deterministic seed/frontier order,
duplicate elimination, snapshot binding, hop/endpoint/edge limits,
truncation, cycles, disconnected endpoints, and historical retractions across
reference and durable stores. A fixed-output query benchmark must remain
independent of unrelated history depth.
