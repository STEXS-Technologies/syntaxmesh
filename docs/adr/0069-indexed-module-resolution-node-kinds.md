# ADR-0069: Index module-resolution node kinds in SQL stores

- Status: Accepted
- Date: 2026-09-28

## Context

The module resolver needs only module declarations and typed import/export
occurrences. Turso currently reads every node payload and deserializes it to
discard unrelated node kinds. This makes a small resolution lookup proportional
to the complete repository graph. The public `Node` representation and its
bincode encoding must remain canonical and unchanged; SQL read columns are
derived projections that must be checked against that payload.

## Decision

Add a database-private `node_kind` integer projection to SQL node rows, with a
composite `(node_kind, id)` index. Assign explicit, append-only codes in the
storage port; code zero is reserved for an unclassified/legacy row and is never
valid after migration. Module, import, and export codes are used to select the
resolver inventory in SQL. Both SQLite and Turso migrations decode and backfill
existing rows transactionally, and their open/integrity validation checks the
projection against the canonical payload. Delta upserts maintain the projection
in the same transaction as the payload. The Turso resolver query reads only the
three relevant kinds; SQLite remains the in-memory reference backend while
maintaining and validating the same SQL projection.

This does not change `Node`, public DTOs, stable IDs, serialized facts, graph
roots, or query semantics. The numeric codes are storage implementation
details, not a wire format.

## Consequences

- Turso no longer scans/deserializes unrelated node payloads for module
  resolution inventory reads.
- SQL migrations must be ordered, checksum-verified, atomic, and backfill old
  rows before advancing the schema version.
- Future `NodeKind` additions must receive a new explicit storage code without
  reusing prior values; the canonical node payload remains authoritative.
- SQLite/Turso conformance and migration tests must cover backfill, updates,
  and query selection.

## Alternatives considered

- Rewriting the resolver to replay module nodes from history: rejected because
  it duplicates canonical state and makes routine resolution depend on history.
- Keeping the full-payload scan: rejected because it is O(all nodes) work for a
  bounded inventory query.
- Changing the public node serialization or enum discriminants: rejected as an
  unnecessary public/durable-format migration.
