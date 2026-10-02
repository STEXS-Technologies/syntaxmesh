# ADR-0040: Explicit SQLite migration before store open

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0037 made schema transitions ordered and transactional, but
`SqliteGraphStore::open` still executed them implicitly. Open also created
indexes, seeded a legacy history anchor, and rebuilt change-event projections.
That means an ordinary application startup can change an existing database
before it restores the store. Shardline's migration contract separates an
explicit migration command from startup's read-only compatibility check.

## Decision

1. Add an explicit `SqliteGraphStore::migrate(path)` operation. It may create a
   fresh schema or apply registered, sequential upgrades, and owns all schema
   and derived-projection backfills.
2. `SqliteGraphStore::open(path)` does not create a missing database, advance a
   schema, create indexes, seed history, or backfill projections. It opens an
   existing database and fails closed unless the schema version, migration
   ledger, required indexes, and logical store state are current and valid.
3. Move the currently implicit history-anchor and change-event reconciliation
   into the final registered migration. Fresh bootstrap creates the complete
   current schema and indexes atomically.
4. Keep migrations one-transaction-per-version with the existing checksum,
   interruption, rollback, and concurrent-opener protections. Operators must
   run migration before opening a stale store; no automatic repair on startup.

## Consequences

- Startup behavior is predictable and non-migrating; upgrades are visible,
  testable operations that hosts can schedule before serving requests.
- This changes the SQLite adapter lifecycle API and requires callers/tests to
  invoke `migrate` for new stores and before upgrading old ones.
- Migration failure leaves the previous schema version and published graph
  readable; retry resumes from the last committed migration.
- No automatic destructive or best-effort compatibility repair is permitted.
