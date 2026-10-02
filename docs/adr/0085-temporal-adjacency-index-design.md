# ADR-0085: Persistent generation-scoped temporal adjacency indexes

- Status: Accepted
- Date: 2026-09-29

## Context

SyntaxMesh has append-only accepted deltas, temporal fact versions, and a
content-addressed persistent fact map for each accepted generation. Durable
`GraphAt(g)` selects that generation's root without replaying intervening
deltas. `historical_node(g, id)` is also an indexed point read. There is no
equivalent bounded historical neighbor query: the current `neighbors` query
uses a generation-tagged in-memory adjacency projection, while constructing
that projection for an old generation materializes the requested graph.

The product source of truth requires bounded historical traversals and
precomputed lineage indexes. ADR-0027 defines “O(1) over history depth” as no
replay/scan of intervening generations for common indexed reads, not strict
constant time independent of index size or result size. ADR-0076 deliberately
dropped temporal endpoint B-tree indexes because live cascades had a cheaper
current-projection path and no temporal neighbor consumer existed. Restoring
those indexes alone would make an important read possible but would scan the
endpoint's old edge versions under churn and would charge every edge version
for two more B-tree writes.

The Rust prototype also exposed a persistence detail: the existing dirty-page
collector follows structural child links, but an outer endpoint page stores
its inner root as a value. A publisher must therefore pin/drain both the outer
root and each changed endpoint's final inner root; draining only the outer
root silently drops newly-created inner pages. Intermediate inner roots for
the same endpoint should be pruned before persistence.

Shardline's useful precedent is its explicit, ordered, transactional migration
lifecycle and operator-controlled startup/upgrade split, already adopted by
SyntaxMesh. Shardline does not provide a historical graph adjacency structure
to copy; the index itself is domain-specific.

## Proposed decision

Add a persistent secondary incidence map, versioned by accepted generation,
alongside—not inside—the canonical fact map. Reuse the existing ordered
content-addressed fact tree as a nested map: the outer tree maps
`(direction, endpoint_id)` to an inner tree root; the inner tree is a persistent
ordered set of edge IDs. Source and target incidence use separate direction
tags so incoming/outgoing queries and weakly connected traversal have explicit
semantics. Each accepted generation records the incidence-map root. Publication
updates canonical facts, both incidence directions, graph manifest, and history
in the same transaction. This avoids inventing a second page format or
rewriting a high-degree endpoint's entire adjacency list after one edge change.

A historic endpoint query resolves the generation and incidence root, seeks
the endpoint in the outer tree, and walks only a bounded ordered range in its
inner edge-ID tree. Edge payloads are fetched from the canonical generation
root. It must not replay deltas, materialize the complete graph, or scan prior
versions.

The runtime-neutral store/query contract should return deterministic,
generation-pinned pages with an opaque cursor and explicit truncation/continuation
state. Higher-hop traversal composes bounded pages and enforces the source-of-
truth bounds (generation range, max hops, max changes/entities, and relation
classes); it must never silently turn an interactive request into an unlimited
history scan. SQLite and Turso are the canonical durable implementations.
InMemory/File remain correctness reference paths and may reconstruct from their
retained history.

The database-independent Rust store crate exposes nested incidence-tree
mutation/page primitives. SQLite and Turso publish generation-scoped incidence
roots atomically with graph generations. SQLite migration v18→v19 and Turso
migration v21→v22 rebuild roots transactionally from retained anchors and
deltas through their ordered migration registries and explicit-migrate/read-only-open
lifecycles. Store-level tests cover direction separation, bounded ordered
edge-ID pages, root checks, removal, unchanged-endpoint sharing, multi-root
dirty-page retention, and cold loading/resumption for a high-degree endpoint.
Cross-backend durable page reads and the runtime-neutral generation-pinned
neighbor-page contract are implemented under [ADR-0086](0086-generation-pinned-historical-neighbor-pages.md), including cursor-bound query paging
and File/Turso CLI equivalence. Fixture-specific tree-cache/page/fact-row
operation counts are now measured per [ADR-0090](0090-benchmark-historical-index-read-counts.md)
and the resumable-read optimization is recorded in
[ADR-0091](0091-resumable-persistent-adjacency-reads.md). This provides direct
evidence for one fixed 10-edge page across the tested retained-history and
unrelated-graph sizes; SQL statement counts, representative workloads, and
multi-hop historical operation scaling remain open. Bounded result tests alone
are not a performance proof.

## Alternatives rejected

- **Replay deltas per neighbor request:** query cost grows with retained
  history and violates ADR-0027.
- **Materialize `GraphAt(g)` and use current adjacency:** cost grows with full
  graph size even for one endpoint.
- **Reinstate temporal source/target B-trees as the final index:** useful as a
  diagnostic/prototype baseline, but one high-churn endpoint may scan many
  expired versions and writes pay the index cost for all temporal edges.
- **One serialized neighbor list per node per generation:** a single edge edit
  rewrites a high-degree node's entire list, defeating changed-facts-only
  publication.

## Consequences

- Historical point-neighbor queries become independent of the number of
  intervening generations, with work bounded by index seek plus returned
  incidence count. Multi-hop work remains bounded by requested output and
  explicit traversal limits; no strict O(1) claim is made for arbitrary output.
- Each edge insertion/removal changes source and target incidence paths and
  adds persistent pages. This write, storage, migration, and integrity-check
  overhead must be measured against representative Shardline/Sim corpora before
  the design is accepted.
- Existing canonical graph roots and stable fact IDs remain unchanged; the
  incidence root is a derived, validated query index, not semantic graph state.
- Existing SQLite/Turso endpoint-index removal remains correct for live
  cascades. It is not reversed by this proposal.

## Acceptance evidence required

- Rust unit/property tests for outer endpoint-key and inner edge-key ordering,
  deterministic page identity, structural sharing, source/target symmetry, and
  pagination.
- SQLite/Turso differential fixtures across add, update, endpoint change,
  removal, reintroduction, migration, reopen, and integrity verification.
- Fixture-level operation-count evidence for a fixed bounded page is recorded
  in ADR-0090/0091; broaden it to SQL statement counts and representative graph
  shapes before making a general scale claim.
- Representative read/write/storage measurements versus the current design and
  the temporal endpoint B-tree diagnostic baseline.
- No non-Rust runtime or out-of-scope analyzed source format is introduced.
