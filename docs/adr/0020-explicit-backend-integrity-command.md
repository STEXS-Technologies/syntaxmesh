# ADR 0020: Run backend integrity checks explicitly

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

Engine status already recomputes logical graph-root and reference invariants.
SQLite and Turso validate the persisted graph root during open/restore, but
neither path exposes a full physical database integrity check to operators.
Such a check can scan the whole database and should not be hidden in routine
status reads, especially while graph status already materializes the full
generation.

## Decision

Add a store-level `BackendIntegrityCheck` port with a typed report, and expose
explicit `integrity` and `integrity-turso` CLI commands. The SQLite and Turso
adapters run their backend's full read-only `PRAGMA integrity_check`; every
returned row is preserved, and only the single `ok` result is success. The
FileGraphStore reports whether its snapshot can be decoded and still matches
the open in-memory state; this is a serialization check, not a database page
check. A failed report produces non-zero CLI exit status. Routine `status`
remains bounded to its existing logical-generation checks and does not run the
full backend scan.

## Consequences

- Backend-specific database checks remain behind store adapters, not in core,
  public DTO, engine, or host-specific graph semantics.
- Operators can request a thorough database scan explicitly and distinguish it
  from the engine's logical graph integrity result.
- File snapshots receive an honest format/open-state check without being
  mislabeled as SQL databases.
- Physical integrity checks may be expensive on large databases; they are
  intentionally separate from `status`.
- This check does not guarantee crash durability, WAL backup correctness,
  graph freshness, or restoration of external extension/runtime inputs.

## Alternatives considered

- Run the full database check on every status request: rejected because it can
  be expensive and would silently turn routine status into a full-store scan.
- Put SQL pragmas in the engine: rejected because it would leak backend
  specifics into the runtime-neutral application layer.
- Report success based only on successful open/root restoration: rejected
  because those checks do not verify the backend's full database structure.

## Migration and compatibility

This adds store integrity report types and a read-only check port plus CLI
commands. It changes no graph data, store schema, or normal publication path.
