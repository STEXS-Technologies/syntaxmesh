# ADR-0037: Ordered, atomic SQLite schema migrations

- Status: accepted
- Date: 2026-09-27

## Context

The SQLite adapter initializes every current-schema table before it reads the
stored schema version, then uses a handwritten branching chain to apply legacy
upgrades. This makes the migration boundary difficult to reason about: opening
an old database mutates it before compatibility is known, version 1 is advanced
outside its migration transaction, and the supported upgrade path is encoded
in duplicated control flow. Shardline's local SQLite store uses checked-in,
versioned SQL, explicit migration metadata, and one transaction per migration
and its applied-migration ledger entry.

## Decision

SyntaxMesh SQLite will use Shardline's explicit, ordered migration discipline,
adapted to SyntaxMesh's data-transforming historical upgrades:

1. Inspect the existing database before creating current-schema objects. A new
   database is bootstrapped atomically; an existing database is upgraded only
   through registered, sequential migration steps.
2. Represent every version transition in one ordered registry with a stable
   version/name and a single apply function. Data migrations may use Rust;
   schema-only additions should use checked-in SQL scripts. No implicit
   fall-through or duplicate version branches.
3. Apply exactly the next migration in an immediate transaction. Commit the
   schema version in the same transaction as the migration's DDL/data changes.
   A failed step leaves its prior version and database state intact.
4. Fail closed on unknown or unsupported schema versions. Migration registry
   entries are contiguous and monotonically ordered; tests enforce this.
5. Record each applied migration in `syntaxmesh_schema_migrations` in the same
   transaction as its schema/data changes. The ledger has stable version/name,
   an application timestamp, and an `adopted` marker. Existing databases and
   fresh databases bootstrapped from the current schema adopt prior registered
   steps with unknown timestamps; newly executed upgrades receive timestamps.
   On open, validate that ledger entries exactly match the registered history.
   SQL-backed migrations also record and validate a BLAKE3 checksum of their
   bundled SQL, following Shardline's checksum discipline. Rust-only data
   migrations keep a null checksum: hashing a function pointer would not
   establish implementation immutability.
6. Keep migration separate from normal store open/restore logic. Restore and
   integrity validation begin only after migration succeeds.
7. Configure a bounded SQLite busy timeout before schema inspection. If a
   migration attempt fails but a subsequent version read proves another opener
   committed a newer schema version, continue from that version; if the version
   did not advance, return the original migration failure. If a concurrent
   fresh-schema bootstrap wins after this connection's initial empty-database
   check, validate its committed migration ledger and accept it only when it is
   the current registered schema. This uses SQLite's database writer lock, not
   a process-local mutex, so independent processes follow the same rule.
8. Test migration interruption with typed, test-only failpoints immediately
   before and after commit. A pre-commit interruption must roll back DDL/data
   and preserve the prior schema version; a post-commit interruption must be
   reconciled by observing and validating the committed schema advance. No
   failpoint or interruption control is present in production builds.

Historical upgrades remain supported from the currently documented earliest
schema. This ADR does not alter graph identities, temporal query semantics, or
the independent Turso schema.

## Alternatives considered

- Keep the current switch/branch chain and only add tests: rejected because it
  preserves the implicit and pre-mutating migration boundary.
- Use a third-party migration framework: rejected for now; the adapter already
  uses rusqlite directly, and its existing upgrades include Rust-level graph
  history transforms that need explicit transaction access.
- Rewrite all historical data migrations as SQL: rejected; several rebuild
  temporal indexes by decoding and validating canonical Rust payloads.

## Consequences

- Every future SQLite schema change must add one immutable registered
  migration and upgrade/rollback fixtures. CI also compares SQL migration
  files, their `include_str!` references, and registered target/name pairs so
  orphaned scripts and registry drift fail before runtime. Editing the SQL of
  an already-applied SQL-backed migration causes store open to fail closed on
  checksum mismatch; Rust migration behavior remains protected by review and
  migration fixtures rather than a misleading function hash.
- The singleton schema-version row remains the current-schema cursor; the
  applied-migration ledger provides auditable ordered history. Checksums are
  populated for SQL-backed migrations and unknown for Rust-only migrations.
- Simultaneous open/upgrade attempts may serialize on SQLite's writer lock. A
  loser can recover only by observing a committed version advance and then
  finishing the registered path; an unchanged or invalid version remains an
  error.
- Typed migration failpoints exercise the exact transactional boundaries
  without introducing production fault-injection machinery.
- ADR-0040 further separates this runner from ordinary store startup: the
  registered migration operation is explicit, while `SqliteGraphStore::open`
  validates compatibility and never upgrades or repairs the database.
