# ADR-0060: Resume persistent-tree mutations after lazy page loads

- Status: accepted
- Date: 2026-09-28

## Context

SQLite and Turso apply delta mutations to content-addressed generation roots.
The shared tree operation may return a missing durable page; both adapters load
that page into the per-operation cache and retry the operation. The retry
currently restarts the entire mutation batch from its original root. Profiling
of a Penelope repository benchmark found persistent-tree insertion, cache
updates, BLAKE3 hashing, and byte comparisons among the dominant CPU costs.
Replaying already-completed mutations after each lazy page load multiplies
those costs by the number of missing pages.

## Decision

1. Add a resumable mutation cursor to `syntaxmesh-store` that advances at most
   one ordered fact mutation at a time. If that mutation requires an unloaded
   page, return the missing-page identity without advancing the cursor or
   discarding the working root/cache.
2. SQLite and Turso adapters load the requested page, resume at the same
   mutation, and continue. Completed mutations are not replayed.
3. Keep the existing batch `PersistentFactTree::apply` API and use it for
   in-memory callers and empty-root sorted bulk construction. Durable stores
   retain their existing transaction, publication, integrity, and rollback
   boundaries.
4. Prune superseded dirty pages at the existing bounded interval while
   retaining all pages reachable from the in-progress root.

## Alternatives considered

- Continue retrying the full batch: rejected because earlier mutations and
  their path hashing are repeated for every newly loaded page.
- Load the entire old fact tree before applying a delta: rejected because it
  changes an indexed incremental path into an O(F) read and defeats structural
  sharing's working-set benefit.
- Change the SQL schema or tree format: rejected; the issue is mutation
  orchestration, and no persisted representation change is needed.

## Consequences

- Persistent tree updates preserve progress across lazy database page reads;
  logical roots and durable page formats remain unchanged.
- Tests must compare resumed updates with the existing batch result across
  upsert, replacement, and removal mutations, and exercise multiple missing
  pages without advancing a failed mutation.
- Re-run representative repository benchmarks before claiming improvement;
  one-run `perf` samples identify a target but do not establish a speedup.

## Implementation and validation

Implemented in the shared store crate and wired into SQLite and Turso. Focused
store/backend tests and `cargo make ci` pass. A three-run Penelope benchmark
shows the expected incremental-path improvement; its one-run pre-change
baseline is not a statistically matched comparison, so this is evidence of a
strong local signal rather than a stable speedup claim. See the benchmark note
in `V0_ARCHITECTURE_PLAN.md` and the ignored raw evidence bundle.
