# ADR-0046: SQLite migration status and operator commands

- Status: accepted
- Date: 2026-09-28

Implementation status: Implemented in the SQLite status API and the
`sqlite-migration-status` / `sqlite-migrate` CLI commands.

## Context

ADR-0040 separates SQLite migration from ordinary store open, but hosts must
currently write a small Rust setup program just to migrate a local database or
inspect its registered state. Shardline's migration workflow provides a useful
operator pattern: an explicit upgrade command and read-only ordered status.

SyntaxMesh cannot safely copy Shardline's general `down` operation. Several
historical migrations transform canonical graph history in Rust and have no
inverse. The currently reversible v13→v14 SQL step creates authored
consequence history; applying its down SQL to a populated store would discard
that data.

## Decision

1. Expose `SqliteGraphStore::migration_status(path)` as a read-only operation.
   It reports the observed schema version, target version, ordered registered
   migrations with applied/pending state, and whether an applied-migration
   ledger was validated. It does not initialize, migrate, repair, or create a
   missing database.
2. Add `sqlite-migration-status <database>` to print that report and
   `sqlite-migrate <database>` to perform the existing explicit upgrade before
   printing the resulting report.
3. Preserve the existing one-transaction-per-version runner, registry,
   checksum checks, interruption reconciliation, and fail-closed `open`.
4. Do not add a generic migration `down` command. SQL down files remain useful
   as tested schema-fixture tools; they are not proof that live canonical data
   is disposable. A future rollback capability requires data-preserving or
   explicitly destructive semantics and a separate contract.

## Consequences

Operators can inspect and upgrade local SQLite databases without embedding
custom Rust setup code. Status remains safe to run against absent, fresh, old,
and current database files. Migration remains a separately invoked mutation;
ordinary open is unchanged.
