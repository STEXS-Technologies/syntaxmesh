# ADR-0086: Expose generation-pinned historical neighbor pages

- Status: Accepted
- Date: 2026-09-29

## Context

ADR-0085 establishes generation-scoped persistent incidence roots and bounded
ordered edge-ID seeks. SQLite and Turso now maintain those roots atomically,
but callers cannot consume them. A public API must carry generation, endpoint,
direction, and continuation state together; otherwise a cursor could be
accidentally reused against a different historical graph or endpoint.

The source-of-truth requirements call for bounded historical traversal and
explicit truncation. Existing SyntaxMesh page patterns use typed cursors and
`has_more`, while the source-of-truth temporal-query rules prohibit unbounded
interactive history scans. The Change Engine remains a separate project and
does not own this read contract.

## Decision

- Add a runtime-neutral store operation for one endpoint's ordered incident
  edges at an exact retained generation. Direction is explicit (`outgoing` or
  `incoming`); a weak-neighborhood caller composes both pages rather than
  relying on an implicit direction convention.
- Durable stores seek the generation's incidence root and return at most
  `limit + 1` edge facts ordered by stable edge ID. They do not replay history,
  materialize the generation, or scan unrelated facts. InMemory/File may use
  the reference snapshot path.
- Add a typed public continuation cursor bound to generation, endpoint,
  direction, and last returned edge ID. Reject a cursor whose bound query
  fields differ from the request. A page reports `has_more`; `next_cursor` is
  present iff another page exists. Page limits are positive and capped.
- The query service resolves each edge's opposite endpoint in the same pinned
  generation and returns deterministic edge/endpoint pairs. An endpoint absent
  at that generation is a valid empty page; a missing generation is an error.
- Expose the operation through the Rust CLI as NDJSON with versioned item and
  footer records. It is a read-only query and does not change publication,
  workflow, verification, or host-runtime boundaries.

## Alternatives rejected

- Replaying deltas or materializing `GraphAt(g)` per page violates depth-
  independent bounded reads.
- A naked edge-ID cursor can be reused for the wrong endpoint, direction, or
  generation and is therefore not the public cursor contract.
- Returning all neighbors in one vector makes high-degree endpoints an
  unbounded interactive operation.
- Putting traversal in a host/transport crate would make semantics runtime-
  specific and prevent embedded callers from sharing the same query behavior.

## Consequences

- The common historical endpoint page cost is an index seek plus work
  proportional to the returned edges, independent of intervening generation
  count. It is not strict O(1) in degree or output size.
- Higher-hop traversal composes bounded pages and must additionally enforce
  generation-range, hop, entity/change, relation, repository, and evidence
  bounds from the source of truth.
- Store adapters and public NDJSON schemas must be tested across SQLite, Turso,
  and the reference implementations, including cursor mismatch, removal,
  reintroduction, endpoint changes, restart, and migration.

## Acceptance evidence

- Implemented: SQLite and Turso page reads resolve the generation-scoped
  incidence root and fetch only the bounded edge IDs and their exact temporal
  payload versions. Store conformance checks ordered bounded continuation on a
  high-degree endpoint; query tests cover cursor binding; a CLI fixture checks
  temporal NDJSON continuation and File/Turso result equivalence.
- Implemented separately: incidence-root migration/backfill and atomic root
  publication are exercised by SQLite and Turso migration/restart tests.
- Measured (2026-09-29): the Shardline-style `benchmark-temporal` harness now
  times fixed 10-edge pages over 64/256/1,024 retained generations and over
  100/1,000/5,000-node graphs where the selected endpoint degree stays 64.
  SQLite/Turso medians were approximately flat across history depth and grew
  modestly with unrelated graph size in this single synthetic run; see the
  exact results/environment in `V0_ARCHITECTURE_PLAN.md`. This is latency
  evidence, not an operation-count or complexity proof.
- Instrumented repeat (2026-09-29, [ADR-0090](0090-benchmark-historical-index-read-counts.md)):
  before cursor resumption, the fixed 10-edge page loaded 18 incidence-tree
  pages but performed 182 cache lookups at each of 64/256/1,024 retained
  generations; it loaded 24/29/29 pages and performed 305/355/355 lookups at
  100/1,000/5,000 graph nodes. The resumable reads in
  [ADR-0091](0091-resumable-persistent-adjacency-reads.md) reduced those lookup
  counts to 47 at every tested history depth and 59/69/69 across graph sizes,
  with the same page-load counts and exactly 10 edge payload rows. InMemory
  materialized one reference snapshot. [ADR-0090](0090-benchmark-historical-index-read-counts.md)
  adds adapter-call-site SQL statement counts: 30 for the history fixture and
  36/41 for 100/1,000/5,000-node graph fixtures. These are direct fixture
  counts, not proof of strict O(1) work in total graph size.
- Still required: broaden fixture/workload shapes, measure high-degree page
  continuations, and validate representative repository
  impact.
- Still required for broader history traversal: a bounded multi-hop API with
  generation-range, hop, entity/change, relation, repository, and evidence
  caps, as specified by the source-of-truth docs. This ADR implements only one
  endpoint page.
- Confirmed: no non-Rust execution runtime or out-of-scope analyzed source
  format was added.
