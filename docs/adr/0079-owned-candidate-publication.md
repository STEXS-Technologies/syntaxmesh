# ADR-0079: Consume staged candidates during atomic graph publication

- Status: accepted
- Date: 2026-09-29

## Context

SQLite publication follows a stage-persist-publish boundary: clone the current
in-memory projection, apply a graph/lineage/consequence transition to that
candidate, persist it in one SQLite transaction, then replace the live handle
only after commit. This preserves the prior usable generation on validation,
write, or commit failure and is consistent with the explicit-transaction
reliability pattern used by Shardline's local SQLite stores.

The shared `InMemoryGraphStore::apply_delta_with_lineage` and
`apply_delta_with_consequences` currently clone their receiver again to provide
their own atomicity. Consequently, SQLite clones its full in-memory state once
at the adapter boundary and again inside the shared store. Three isolated
SQLite runs on the 696-file Shardline Rust corpus measured the incremental
adapter clone at 197 ms median and the nested shared-store clone at 352 ms
median, for a one-file update. The same benchmark measured the inner graph-delta
work at 52 ms and SQL persistence at 336 ms. These nested stage timings are
separate intervals and must not be summed as if they were exclusive.

## Decision

1. Preserve `GraphStore::apply_delta_with_lineage` and
   `GraphStore::apply_delta_with_consequences` as atomic in-place operations:
   on error, the receiver remains unchanged.
2. Add consuming staging operations on `InMemoryGraphStore` for graph deltas
   with lineage and consequences. A staging operation takes ownership of an
   already-detached candidate and returns the fully validated candidate plus
   its manifest. It does not mutate or publish into another live store handle.
   If staging fails, the consumed candidate is dropped; callers keep their
   original live state untouched and must not recover or publish a partial
   candidate.
3. Implement the existing atomic `GraphStore` operations by cloning once,
   invoking the consuming stage operation, then replacing the receiver only
   after success. SQLite adapters may invoke the stage operation on their
   already-detached candidate, persist it transactionally, and publish it to
   the live handle only after commit. Turso already stages publication directly
   in its SQL transaction and does not use this in-memory candidate path.
4. Keep database transactions and host/workflow concerns out of
   `syntaxmesh-store`; staging remains a deterministic in-memory graph
   operation. Durable adapters retain responsibility for their own transaction
   and commit/reconciliation semantics.
5. Apply the same staging semantics to graph-only, explicit-lineage, and
   explicit-consequence publication in the InMemory/File/SQLite paths; do not
   leave the latter two with nested full-state clones. Turso retains its
   transaction-native implementation.

## Alternatives considered

- Remove the adapter candidate clone and mutate the live in-memory projection
  before SQL commit: rejected because a database failure would expose memory
  state that was never durably committed.
- Weaken the public `GraphStore` atomicity guarantee: rejected; callers and
  adapters rely on failure leaving the current generation intact.
- Add copy-on-write/persistent maps or a new third-party collection: deferred.
  The measured redundant clone can be removed without replacing the established
  ordered-map representation or adding a dependency.
- Retain both candidate clones: rejected as unnecessary full-state work on the
  measured incremental path.

## Consequences

- Existing callers retain the same atomic publication behavior.
- Adapters can avoid the inner clone while retaining the outer detached
  candidate and persist-before-publish rule.
- The consuming staging methods are public on the concrete reference store so
  adapter crates can use them; their ownership/drop-on-error contract must be
  explicit in API docs and tested. They do not alter serialized data or the
  `GraphStore` trait contract.
- Benchmark stage names must distinguish adapter cloning, shared graph staging,
  SQL transaction persistence, and final handle publication. Nested stages are
  reported separately and are not additive.

## Verification

- Failed graph, lineage, and consequence validation leaves the receiver and
  durable database generation unchanged.
- File and SQLite persist a staged candidate and publish the in-memory handle
  only after durable replacement/transaction commit; injected persistence
  failures leave old and reopened state identical. Turso continues to stage and
  commit directly in its own SQL transaction.
- InMemory, File, SQLite, and Turso results remain differentially equivalent,
  including idempotent retries and restart/history reads.
- Verification completed on 2026-09-29: the same isolated Shardline SQLite
  corpus was measured in three fresh-database runs before and after. Median
  one-file incremental indexing moved from `1,944 ms` to `950 ms` (~51% lower);
  `sqlite.candidate_graph_delta` moved from `902 ms` to `51 ms`. The nested
  shared-store candidate clone (`352 ms` before) is absent afterward; the one
  required SQLite adapter clone remained ~`204 ms`. Persist time was `363 ms`
  after versus `336 ms` before, so this improvement is attributable to removing
  redundant candidate work, not faster SQLite persistence. Initial indexing
  moved from `31.712 s` to `29.384 s`; peak RSS moved from `3,606.9 MiB` to
  `2,260.2 MiB`, but the latter changes are observational and are not claimed as
  clone-removal effects. Evidence is under ignored
  `target/benchmark-results/repository-20260929T001451.271582Z-uncommitted/`
  and `target/benchmark-results/repository-20260929T002848.379403Z-uncommitted/`.
  This is one repository/host comparison, not an SLA.
