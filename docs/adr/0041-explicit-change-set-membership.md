# ADR-0041: Persist explicit, provenance-backed change-set membership

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0036 requires a first-class logical `ChangeSet` that groups one or more
atomic `ChangeEvent`s, but only through explicit, provenance-backed assignment
in v0. The current event is a deterministic projection of an accepted graph
transition. Rewriting that event when a user later groups it would change its
identity or mutate accepted history; inferring a group from adjacent commits,
time proximity, or co-changed facts would make correlation look like intent.

The source-of-truth document also describes change-set kind, title,
originating intent, parent change sets, external references, repository scope,
generation bounds, and provenance (§233). The first usable slice must preserve
that distinction while fitting the current repository/worktree store scope.
Shardline's persisted reliability evidence provides a useful pattern: keep
typed, append-only evidence separate from the materialized current state and
verify it against its authoritative source. SyntaxMesh's existing
generation-scoped `ChangeEvent` is the authoritative event; membership is a
separate, explicitly asserted temporal relation.

## Decision

1. Add a stable `ChangeSet` identity and a versioned `ChangeSet` fact carrying
   the source document's full kind vocabulary, optional title, typed
   originating-intent node, explicit parent `ChangeSetId`s, separate Git/PR/
   issue/ADR references, repository membership, first/last generation bounds,
   and provenance. Keep the core type host- and database-independent. The
   store's current single-repository/worktree scope remains unchanged; the
   repository list is descriptive and may name repositories outside this
   store. IDs are globally stable, so the same set can have explicitly
   assigned events in separate repository stores; v0 query results are
   store-local and cross-store aggregation belongs to a later query layer.
   Generation bounds are validated against retained store sequence, never by
   comparing opaque `GenerationId` bytes.
2. Represent event membership as its own versioned fact keyed by the logical
   pair `(ChangeSetId, ChangeEventId)`, with the asserting provenance. An
   assignment is created or revised only when explicitly supplied by a caller
   in an accepted generation. Both assignment and removal assertions carry
   their asserting provenance. A `ChangeSet` may span events/generations; an
   event may belong to multiple explicitly declared sets, but a duplicate
   `(ChangeSetId, ChangeEventId)` pair is one versioned relation, not duplicate
   edges.
3. Keep `ChangeEvent` immutable and deterministic from the accepted canonical
   graph transition. Membership declarations are excluded from the event
   identity digest. An accepted delta may declare membership for its own
   resulting event or for a previously retained event; both the graph
   transition and its membership/version-index updates commit atomically.
4. Store a membership change as an accepted lineage-delta fact, not an
   out-of-band mutable record. Its generation-validity interval records when the
   association was accepted. Do not backfill inferred memberships into
   existing history. Old databases migrate to an empty membership projection;
   retained `ChangeEvent`s stay unchanged.
5. Build durable indexes by `(ChangeSetId, valid-generation)` and by
   `(ChangeEventId, valid-generation)`. `EventsForChangeSet` is a bounded,
   cursor-paged query with a cursor bound to its `ChangeSetId`; it reads the
   membership index and then the indexed event rows, never replaying all
   generations. A point `ChangeSetAt(id, generation)` reads the declaration
   version valid at that exact retained generation through the same validity
   index and returns its validity interval. InMemory/File may derive the
   reference answer from retained history.
6. Require provenance references to resolve to canonical provenance facts in
   the accepted state. Reject assignments to unknown events or change sets,
   stale generations, and conflicting revisions to the same active membership
   fact. Rejected publication changes neither graph state nor lineage
   projections.
7. Do not infer membership from Git commit adjacency, a shared fact, common
   provenance producer, branch/PR naming, or timestamps. Future declared
   heuristics must use a distinct assignment-basis type and remain distinguishable
   from explicit assertions. Membership alone is not causation; consequence
   edges remain a separate ADR-0036 concern.

Concrete DTO field names, cursor encoding, and additive SQLite/Turso migration
versions are fixed by the implementation fixtures before release; the semantic
contract above is fixed now. The implementation remains within
`syntaxmesh-core`, `syntaxmesh-api-model`, the store port/adapters, and query
service. It does not move mutation workflows into SyntaxMesh or Change Engine
logic into the graph engine.

## Alternatives considered

- **Put `Option<ChangeSetId>` on `ChangeEvent`:** rejected because later
  assignment would mutate a deterministic accepted-transition record and
  because one generation event can be grouped through later, separately
  accepted evidence.
- **Infer sets from commits, co-change, or timing:** rejected because these
  are at most heuristics and cannot represent explicit intent or support
  auditable provenance.
- **Write assignments as opaque durable records:** rejected because it would
  bypass canonical fact versioning, atomic generation publication, and
  history-depth-independent indexed queries.
- **Model `ChangeSet` as an engine workflow:** rejected; the Change Engine owns
  mutation workflows. SyntaxMesh stores and queries accepted evidence only.

## Consequences

- Implemented foundation: core types, atomic File/SQLite/Turso lineage
  publication, append-only generation journal, versioned membership indexes,
  empty-lineage migration backfill, bounded fixed-snapshot query, and
  schema-v6 temporal NDJSON records. SQLite/Turso keep membership projections
  in the same transaction as graph publication; File snapshots use an explicit
  versioned envelope with legacy bincode compatibility. Existing ChangeEvent
  identity and payload remain independent of ChangeSet declarations.
- Prepared graph-plus-lineage publication now uses the runtime-neutral
  workflow contract and Penelope's durable operation journal (record v3);
  legacy v1/v2 operation records decode with empty explicit lineage. The Engine
  accepts a prepared request without taking ownership of the Change Engine's
  mutation workflow, and opt-in StateChronicle generation verification remains
  on that path.
- Remaining release fixtures: unknown set/event/provenance, conflicting
  membership, retry, stale-base, and injected interruption around publication
  need equivalent coverage across durable adapters. A fixed-output history-depth
  benchmark and write/storage-amplification accounting are also release gates.
- This slice does not close consequence-edge or bitemporal-state-selection work
  from ADR-0036/ADR-0031, nor does it add ChangeSet authoring to the indexer or
  move mutation workflow into SyntaxMesh. [ADR-0048](0048-cli-changeset-history-queries.md)
  now exposes read-only, generation-pinned declaration and event-membership
  queries in the File/Turso CLI. Product-facing ChangeSet authoring remains a
  separate scope decision.
