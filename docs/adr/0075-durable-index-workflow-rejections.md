# ADR-0075: Retain actionable rejected indexing workflow diagnostics

- Status: Accepted
- Date: 2026-09-29

## Context

SyntaxMesh persists indexing workflows as Penelope records, including their
terminal rejected phase and saga events. Read-only diagnostics currently expose
only aggregate prepared/completed/rejected counts. Once a process exits, an
operator cannot inspect which run was rejected or the durable cause. Gate 4 of
the [v0 architecture plan](../V0_ARCHITECTURE_PLAN.md) explicitly leaves
host-owned durable failure history open.

Shardline's reliability work demonstrates the value of durable, typed lifecycle
evidence and bounded verification/read paths. SyntaxMesh already has the
appropriate durable lifecycle owner: Penelope. Adding a second event log would
duplicate the workflow journal and create another consistency boundary.

## Decision

1. Keep rejected indexing workflow evidence in the existing Penelope operation
   record. Do not create a separate failure table, graph fact, StateChronicle
   record, or external log as the source of truth.
2. Version the private Penelope record encoding additively and persist a typed
   terminal rejection reason alongside the existing saga events. Preserve
   decoding of prior record versions; historical rejected records without a
   typed reason remain explicitly unknown rather than being guessed.
3. For the current terminal rejection path (stale generation base), persist the
   expected and observed generation IDs as structured values. Do not turn
   transient storage, verification, or infrastructure errors into terminal
   rejections; those remain retryable/recoverable workflow failures.
4. Expose a read-only, bounded diagnostic query through the embedded engine and
   CLI, including run ID, rejection category, and its typed context. Listing
   diagnostics must never replay/recover workflows or mutate records. Bound
   pages and use a deterministic cursor/order so large histories do not become
   unbounded status output.
5. Keep the diagnostic API independent of database and transport types. The
   engine/CLI may adapt the existing durable-record capability; core facts and
   canonical graph history remain unchanged.

This adapts Shardline's typed durable-evidence and bounded-inspection practices
to SyntaxMesh's existing Penelope journal instead of importing Shardline's
database, reliability crate, or runtime.

## Consequences

- Operators can diagnose terminal rejected index runs after restart without
  enabling StateChronicle verified graph history.
- No graph schema, graph root, temporal fact schema, or database migration is
  required if the versioned Penelope payload remains self-contained.
- Adding fields to the persisted Penelope record requires backward-compatible
  record decoding and explicit old/new serialization tests.
- This is diagnostic history, not a cryptographic proof, a complete record of
  every transient failure, or permission for hosts to mutate workflow state.
- The current journal has no rejection-phase index. Reads fetch ordered key
  ranges in bounded batches and keep memory bounded, but filtering may scan the
  full run journal in the worst case; this implementation does not claim an
  O(1) rejection lookup.

## Verification

- A stale-base rejection persists a typed reason and remains inspectable after
  close/reopen; its prepared-to-rejected transition remains CAS-protected.
- Legacy rejected records decode with an unknown reason; prepared/completed
  legacy records retain their existing replay behavior.
- Read-only diagnostics do not recover, rewrite, or compact records.
- Bounded pages have deterministic ordering, cursor continuation, and no
  duplicates across File, SQLite, and Turso durable-record implementations.
- Existing graph generations, roots, and query results are unchanged by
  rejection inspection.
