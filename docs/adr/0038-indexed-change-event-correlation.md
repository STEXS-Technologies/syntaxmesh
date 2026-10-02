# ADR-0038: Expose conservative, indexed change-event correlation

- Status: accepted
- Date: 2026-09-27

## Context

ADR-0036 requires a navigable history graph while forbidding causal claims
based only on event order or co-change. SyntaxMesh already persists each
accepted `ChangeEvent` and an indexed relation from changed canonical fact
identity to event generation. That projection can establish one narrow,
useful relation without replaying generation deltas or adding a second index:
two transitions both directly changed the same canonical fact identity.

## Decision

Add a bounded, paginated `HistoricallyCorrelated` relation between a pinned
source event and later events that directly changed a caller-selected shared
`FactRef`.

1. The query is anchored to one accepted source generation and one fact that
   the source event directly changed. An anchor without an event, or a fact
   absent from that event, is rejected.
2. Candidate target events come from the existing fact-to-event index and are
   strictly after the source cursor. The continuation cursor is bound to the
   source event identity and advances by target generation. Validate that its
   generation is at or after the source in the retained generation sequence
   and that it directly changed the selected fact; generation IDs themselves
   are not ordered. Durable stores perform both validations with indexed
   generation/event lookups. Reads are bounded by the requested page size; the
   reference backends may scan their retained history as documented.
3. Each result identifies the source and target event IDs and the exact shared
   canonical fact identity as its evidence. It is labeled
   `historically_correlated`; it does not claim that one event caused,
   enabled, or depended on the other. Shared identity can span a removal and
   later reintroduction, which remains correlation, not continuity.
4. Do not infer cross-fact relations, group `ChangeSet`s, or create
   transitive consequence paths in this increment. Stronger consequence kinds
   require separate explicit evidence and contracts under ADR-0036.

This query is runtime-neutral. Durable adapters reuse their existing indexed
event lookup; no schema migration or host dependency is introduced.

## Alternatives considered

- Replay deltas to find later events: rejected because cost grows with retained
  history and repeats work already represented by the fact/event index.
- Treat later event order or general co-change as causal: rejected by the
  product's causality discipline.
- Add a second consequence table immediately: rejected for this first narrow
  relation because the existing indexed event/fact projection is sufficient.
- Infer `ChangeSet` membership from adjacent events: rejected; grouping must
  be explicit and provenance-backed.

## Consequences

- Temporal query output can explain this relation using stable event IDs and
  the shared fact identity that supports it.
- This is one conservative edge class, not complete consequence analysis,
  cross-fact propagation, bitemporal state selection, or a claim of causality.
- Tests must cover cursor continuation, anchor validation, same-fact
  reintroduction, serialization, and equivalent results across backends. Benchmarks
  should measure fixed pages as retained history grows.
