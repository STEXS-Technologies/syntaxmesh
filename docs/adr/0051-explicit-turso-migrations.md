# ADR-0051: Explicit Turso migrations and read-only open

- Status: accepted
- Date: 2026-09-28

## Context

SQLite now has a registered migration path, read-only migration status, and an
explicit upgrade operation. The canonical Turso adapter still runs a handwritten
version chain from `TursoGraphStore::open`, executes the latest `CREATE TABLE IF
NOT EXISTS` schema before inspecting the stored version, creates indexes on
startup, and backfills legacy history projections during open. An unsupported or
partially migrated database can therefore be mutated before compatibility is
known, and ordinary engine startup can perform expensive migration work.

Turso is the canonical persistence adapter, so the stronger SQLite lifecycle
cannot remain reference-backend-only. The adapter must preserve the pure
`GraphStore` port and the runtime-neutral engine boundary while adopting the
same explicit, inspectable migration discipline.

## Decision

1. Add `TursoGraphStore::migration_status(path)` as a read-only inspection and
   `TursoGraphStore::migrate(path)` as the only schema-upgrade/bootstrap
   operation. Status must not create a file, enable WAL, create tables, repair
   indexes, or backfill graph history.
2. Make `TursoGraphStore::open(path)` validate an existing current schema and
   its required indexes/projections/root. It must not migrate, create a missing
   database, create indexes, add history anchors, or backfill event projections.
3. Replace inline version branching with one contiguous ordered migration
   registry. Each schema/data transition and its schema-version advancement
   remains atomic; unknown, missing, or future versions fail closed.
4. Add an applied-migration ledger for future migrations. Existing pre-ledger
   history is adopted explicitly, with unknown historical application times;
   SQL-backed steps retain checksums, while Rust data transforms are protected
   by migration fixtures rather than function-pointer hashes.
5. Move legacy history-anchor and change-event backfills into the explicit
   migration/bootstrap operation, outside normal open. Each backfill must be
   atomic and safely retryable. Normal open only validates canonical state.
6. Keep the CLI as a host concern: expose read-only `turso-migration-status`
   and explicit `turso-migrate` commands, and require these lifecycle calls in
   setup/benchmark/test paths before opening a Turso store.
7. Keep migration metadata and execution entirely inside the Turso adapter;
   no database, workflow, transport, or host dependency is added to core,
   public DTOs, the store port, or the engine.

## Alternatives considered

- Keep automatic migration on open because Turso is embedded: rejected; it
  makes application startup a hidden, potentially graph-sized mutation and
  prevents safe read-only inspection of a stale database.
- Make SQLite the only backend with an explicit migration lifecycle: rejected;
  Turso is the canonical backend and must not have weaker schema guarantees.
- Replace Rust migration functions with a generic migration framework: rejected;
  historical steps decode and transform typed canonical graph payloads.

## Consequences

- Hosts must explicitly migrate new or stale Turso databases before opening
  them. CLI commands, benchmarks, and tests make that setup visible.
- Opening a current store is predictable and validation-only. Operators gain
  a read-only status command and a separately invoked, retryable upgrade path.
- Existing Turso databases remain supported through the registered legacy
  transitions; migration fixtures must prove every historical data transform
  and rollback boundary before the automatic path is removed.
- No downgrade command is added. Historical payload/data migrations may have no
  safe inverse.
