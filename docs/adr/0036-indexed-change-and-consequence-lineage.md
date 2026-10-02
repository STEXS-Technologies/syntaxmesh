# ADR-0036: Materialize change and consequence lineage as a temporal graph

- Status: accepted
- Date: 2026-09-27

## Context

SyntaxMesh already retains accepted manifests and deltas, versions canonical
facts, indexes observation/acceptance timelines, and selects durable historical
generation roots without replay. Those layers answer what facts or graph state
existed at a generation. They do not yet provide the v5 history graph: a
queryable, evidence-backed relation between a transition and the facts or later
transitions associated with it. Replaying every generation delta to answer
"where did this fact come from?" or "what later work depended on this change?"
would make this central query grow with total history depth.

The source of truth requires stable logical `ChangeSet` identity, atomic
`ChangeEvent`s, temporal fact identity/versions, consequence classes, explicit
provenance, and a strict distinction between causation and temporal
correlation (§§232–241). The existing generation delta is authoritative input,
but it is not itself a navigable semantic lineage index.

## Decision

Build an indexed temporal lineage projection from accepted generation
transitions. Keep canonical graph facts, manifests, deltas, and provenance as
the source of truth; lineage is a transactionally maintained, rebuildable
projection and never a second mutable authority.

The initial lineage model has these layers:

1. **Change event:** every accepted graph transition has one stable event
   identity, before/after generation, and references to the canonical facts it
   introduced, updated, or removed. Runtime observation-only transitions are
   events too. An event identity is deterministic from repository/worktree,
   transition endpoints, and the canonical transition digest; retries of one
   accepted transition must produce the same identity.
2. **Fact-version links:** a change event links to the exact version/fact
   references changed by its delta. Versions of the same stable fact identity
   may be linked as `supersedes`; equal identity or temporal adjacency alone
   does not claim a causal relationship between different facts.
3. **Logical change set:** a change set groups one or more events across
   generations or repositories. Initial v0 supports explicit, provenance-backed
   assignment only. Do not infer a logical change set from adjacent commits,
   co-change, or time proximity.
4. **Consequence edge:** edges carry a typed class, evidence/provenance, and
   derivation. Direct dependency, generated-artifact, contract/build/test,
   runtime-observed, declared-migration, and historical-correlation relations
   remain distinct. Never infer `caused_by` solely from event ordering.

Persist these projections atomically with their accepted transition in durable
backends. Index change-event lookup by changed fact and generation; index
lineage traversal by source/target, consequence class, and evidence reference.
Maintain the projection incrementally from the accepted delta. Rebuild and
migration may scan retained deltas, but ordinary point and bounded-page reads
must not replay or scan all prior generations. The reference InMemory/File
implementations may derive expected answers by scanning retained history.

Expose runtime-neutral typed query contracts only with cross-backend,
serialization, migration, and cursor fixtures. Point origin/last-change lookup
targets index-depth plus result-size; bounded lineage traversal targets
index-depth plus returned nodes/edges. These are history-depth-independent
costs, not strict O(1) claims. Every traversal must accept explicit depth and
result limits; broad propagation analysis is streamed/scoped through the
analytical layer.

This decision does not implement full bitemporal state selection. Generation
validity, producer observation time, and engine acceptance time remain distinct
per ADR-0031; a lineage event's acceptance time is not the modeled time of its
facts. Historical state at a modeled coordinate and knowledge-time cutoff must
be a separate indexed query contract. Nor does this decision authorize
heuristic causal inference or let the Change Engine's mutation workflow enter
SyntaxMesh.

## Alternatives considered

- **Replay deltas for each lineage request:** rejected; request cost grows with
  retained history and repeats work already needed for index maintenance.
- **Treat `GenerationChange` as the complete history graph:** rejected; a
  delta is an auditable transition payload, but has no stable event/fact-version
  links, logical grouping, consequence evidence, or indexed traversal surface.
- **Infer causes from temporal order or co-change:** rejected; correlation is
  not causation and must be represented with its own explicit class.
- **Store a full semantic graph copy per generation:** rejected; duplicates
  state, amplifies writes, and is unnecessary when immutable roots and indexed
  lineage projections can share canonical facts.
- **Make StateChronicle the lineage query store:** rejected; it verifies
  accepted-generation history but does not replace the query indexes or
  semantic relationships.

## Consequences

- Accepted deltas remain the deterministic rebuild source; lineage updates
  become part of atomic generation publication and integrity validation.
- `ChangeSet`, `ChangeEvent`, fact-version links, and consequence edges need
  stable identity, provenance, explicit temporal interpretation, bounded query
  DTOs, and additive schema migrations before becoming public. `FactVersionRef`/
  `FactSupersedes` links are public and derive from indexed per-fact intervals;
  they omit links over a removal/reintroduction gap. The initial `ChangeEvent`
  projection is now also persisted atomically with accepted transitions and
  indexed by changed fact/generation. It supports per-fact pages and direct
  generation lookup (including no-op transitions), with temporal NDJSON schema
  v4. This is transition membership, not cross-fact causation: logical
  `ChangeSet`s, consequence edges, bitemporal state selection, and their
  evidence-backed traversal are still not implemented.
- Existing `changes` remains a raw generation-transition query. New lineage
  query modes must identify whether an edge is direct, derived, observed, or
  correlated and include evidence sufficient to explain the relation.
- Release gates must include backfilled/restarted databases, delta-only
  publication, no-op generations, observation-only events, rejected writes,
  cursor continuation, cross-backend equivalence, depth-varying fixed-output
  benchmarks, and write-amplification/storage accounting.
- The v0 temporal foundation is not complete until this projection and the
  separate bitemporal state-selection contract are implemented and measured.
