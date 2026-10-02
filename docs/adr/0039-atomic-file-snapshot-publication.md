# ADR-0039: Atomic file snapshot publication

- Status: accepted
- Date: 2026-09-28

## Context

`FileGraphStore` is the restartable reference/conformance backend. Its previous
writer used one fixed sibling `.tmp` name and `std::fs::write`, so concurrent
store instances could truncate or replace each other's staging file and a
process interruption during the write could leave the only staging file
partial. The backend already promises whole-generation publication, while the
operations runbook explicitly does not promise crash durability.

Shardline's local filesystem writers use unique, exclusively created sibling
temporary files, complete writes, file synchronization, atomic rename, and
cleanup on failure. SyntaxMesh adopts the pieces appropriate for its single
snapshot file, without importing Shardline's rooted-path security API.

## Decision

1. Serialize the candidate state before touching disk.
2. Create a collision-resistant sibling temporary file with exclusive-create
   semantics. Retry only on a name collision.
3. Write all snapshot bytes and synchronize the temporary file before rename.
4. Atomically rename the completed sibling into the snapshot path. Remove the
   temporary file when write or rename fails; do not publish the candidate in
   memory on failure.
5. Treat successful rename as atomic visibility, not a cross-platform
   crash-durability guarantee. Parent-directory synchronization and coordinated
   multi-writer locking are not part of this reference backend contract.

## Consequences

- Readers observe the prior complete snapshot or the next complete snapshot,
  not a partially written target; staging names do not collide between writers.
- A failed write/rename leaves the in-memory published generation unchanged
  and attempts to remove its staging file.
- A host/power failure may still lose the renamed directory entry. The runbook
  must continue to distinguish this reference backend from a durable database.
- Tests cover restart and failed replacement cleanup. This does not establish
  concurrent multi-writer safety; keep one writer per store as documented.
