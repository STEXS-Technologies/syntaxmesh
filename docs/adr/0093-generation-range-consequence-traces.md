# ADR-0093: Preserve chronology in bounded generation-range consequence traces

- Status: Accepted
- Date: 2026-09-29

## Context

The v5 source of truth (§§242–244, 297–298) requires a bounded, provenance-
backed explanation of consequences across later generations. The existing
`consequence_neighborhood` is explicitly a fixed-generation query (ADR-0092).
SQLite and Turso already persist consequence-edge validity intervals and
endpoint indexes; replaying every generation or materializing every snapshot
would discard that advantage. Ordinary historical-neighbor pages (ADRs
0085–0086) are generation-root based and describe canonical graph edges, not
consequence assertions, so they cannot substitute for this index.

A union of edges observed anywhere in a requested range is not a valid temporal
trace: it can connect assertions in reverse chronological order, lose when each
edge was actually available, or imply causality merely because two facts were
nearby in time. A delayed later consequence may legitimately follow an earlier
assertion after that earlier assertion was retracted; each hop's own validity
must therefore be retained rather than requiring all hops to overlap in time.
The evidence path must preserve distinct relation kinds.
The current `EvidenceClass` is categorical, not ordered; there is no accepted
trust ranking that would make a numeric or ordinal `evidence_threshold` sound.

## Decision

1. Add a runtime-neutral, bounded page primitive for consequence assertions
   incident to an exact `LineageEndpoint` whose validity interval overlaps a
   caller-specified inclusive generation range. Durable implementations query
   the existing endpoint/validity indexes; they must not replay generation
   deltas. Cursor identity binds endpoint, both range bounds, and last edge ID.
2. Build multi-hop traversal on that page primitive. A path carries a
   non-decreasing `reachable_from_generation`: for each next edge this is the
   later of the prior path's reachable generation and the edge's valid-from
   generation. The edge must be active at that assigned generation and within
   the requested upper bound; prior hops need not remain active at later hops.
   Return the assigned generation and the edge's exclusive valid-until
   generation on every hop. This permits delayed consequences while excluding
   reverse-time joins or an edge already expired before it could be traversed.
   Preserve distinct reachable `(endpoint, generation, depth)` states and
   equal-time alternate assertions as a temporal evidence DAG; do not collapse
   later valid paths merely because an endpoint was reachable earlier.
3. Preserve edge direction, `ConsequenceKind`, evidence class, derivation,
   provenance, and exact endpoint identities on each hop. A trace is an
   evidence-backed historical path, not proof of causation. No relation kind,
   evidence class, or intermediate hop may be silently merged or reworded.
4. Require explicit range, hop, endpoint, edge, and scanned-incidence caps;
   report truncation. Relation-kind and evidence-class filters are sets, not
   implicit ordering. Defer a minimum trust threshold until its taxonomy and
   semantics are independently decided.
5. Keep computation in the runtime-neutral Rust query/store layers. Do not add
   databases/workflows/transports to core or public DTO crates, and do not
   introduce an analyzed language or executable runtime outside ADR-0078.

## Alternatives rejected

- Union edges across the range then run ordinary BFS: permits reverse-time
  paths and loses per-hop validity needed to explain delayed consequences.
- Replay all deltas or call `GraphAt` for each generation: work scales with
  history depth and duplicates indexed data already maintained by the stores.
- Reuse canonical historical adjacency roots: those roots index graph edges,
  not typed, provenance-bearing consequence assertions.
- Interpret `EvidenceClass` as an ordinal trust score: its model documentation
  explicitly says certainty is categorical, not a score.

## Consequences

- This is a read-only historical interpretation; it does not create or mutate
  consequences, ChangeSets, or workflows. The Change Engine remains separate.
- SQL interval/index reads provide depth-independent selection work, but total
  traversal remains proportional to visited incidence pages and returned
  paths, bounded by the request. No strict O(1) claim applies to arbitrary
  output.
- The contract must preserve exclusive end-bound semantics for edge validity;
  the caller's requested generation range itself is inclusive at both ends.
- A trust-class ranking and causal/epistemic interpretation remain explicit
  product decisions, not implementation shortcuts.

## Verification required

- Add cross-backend tests for interval overlap, exact inclusive range bounds,
  cursor binding, removal/retraction, and reintroduction.
- Add chronological path fixtures where a later edge appears after an earlier
  edge was retracted (valid delayed path), plus a reverse-time or already-
  expired-at-reach edge that must not be returned.
- Verify SQLite/Turso query plans use endpoint interval indexes and benchmark
  bounded fixed-output traces across increasing retained history.
- Keep Rust-only execution and the ADR-0078 scanner boundary unchanged.
