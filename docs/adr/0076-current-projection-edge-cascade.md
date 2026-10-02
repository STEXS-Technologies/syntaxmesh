# ADR-0076: Reuse current edge indexes for temporal cascade publication

- Status: Accepted
- Date: 2026-09-29

## Context

SQLite and Turso keep source/target columns and indexes on temporal edge
versions. Live node deletion used those temporal indexes both to discover the
edges removed from persistent generation roots and to close their temporal
versions. Turso's current graph projection has indexed edge endpoints and is
the exact parent state being changed. SQLite instead maintains that parent
adjacency in its validated in-memory reference store. Maintaining and
consulting the temporal endpoint indexes for the same live cascade adds write
and read amplification.

Temporal endpoint indexes are still required while upgrading historical
schemas: earlier migration steps rebuild roots and backfill temporal edge
intervals from retained deltas. They are not required once a store reaches the
current schema, because accepted live cascades can use the already indexed
current projection and publish by stable edge identity.

## Decision

1. On live SQLite publication, use the already maintained in-memory adjacency
   to determine the deduplicated incident edge IDs. The transaction's manifest
   compare-and-swap must still succeed before those IDs are applied.
2. On live Turso publication, read the deduplicated incident edge IDs from the
   indexed current edge projection in the same immediate transaction that
   checks the parent manifest. Use that one set for temporal fact closure,
   persistent-root mutation, and current-projection deletion.
3. Close and remove incident temporal/current edge rows in bounded batches by
   stable edge ID. Do not rediscover live incident edges through temporal
   source/target indexes.
4. Keep endpoint-index queries in pre-current-schema migration/backfill code.
   Add ordered SQLite and Turso migrations that drop the temporal
   source/target indexes only after the legacy migration path has completed.
   Fresh and upgraded databases must end with the same schema; opening never
   repairs or migrates an old schema.
5. Preserve Turso's endpoint indexes on its current edge projection and keep
   consequence endpoint indexes where historical consequence queries use
   them. SQLite's current adjacency remains in memory. No public DTO,
   graph-root algorithm, or query interpretation changes.

## Consequences

- New accepted node-removal deltas no longer maintain temporal edge-endpoint
  indexes and do not query those indexes to compute root mutations.
- A migration from an old database runs all required endpoint-index backfills
  before dropping those indexes in the final transition. A failed transition
  remains transactional and retryable.
- Fact identity/history lookups continue to use the temporal identity index;
  full generation reads continue to use persistent roots. This decision does
  not change historical query complexity or imply a measured latency gain.
- Cross-backend deletion fixtures must verify current and prior graph state,
  exact temporal edge validity intervals, migration from the immediate prior
  schema, and fresh-schema parity.

## Verification

- SQLite and Turso have no temporal endpoint indexes after current migration
  and fresh bootstrap; Turso retains current-edge endpoint indexes, while
  SQLite retains its in-memory adjacency path.
- Immediate-prior schema upgrade is transactional and leaves temporal fact
  history/root reads unchanged.
- A high-degree, multi-node deletion closes each edge once at the accepted
  generation in both durable backends.
- Full architecture, migration, and workspace CI gates pass.
