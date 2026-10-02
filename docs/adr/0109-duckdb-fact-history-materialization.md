# ADR-0109: Transactional DuckDB fact-history materialization

- Status: accepted
- Date: 2026-09-29

## Context

[ADR-0098](0098-typed-analytical-export-boundary.md) keeps analytics downstream
of canonical graph publication and selects typed Rust/Arrow handoff with
Parquet rebuild artifacts. [ADR-0107](0107-generation-scoped-fact-version-changes.md)
and [ADR-0108](0108-bounded-fact-version-change-pages.md) provide the exact,
bounded source feed required for delta-proportional synchronization. The
current DuckDB adapter only queries a verified immutable Parquet export; it
does not maintain a restartable on-disk projection.

Shardline's useful rebuild property is that incomplete or dirty reconciliation
does not authorize destructive cleanup. SyntaxMesh should apply that property
to materialization replacement without copying Shardline's domain storage or
workflow coupling. NDJSON stays an explicit user/tool export and is not used
for materialization or internal handoff.

## Decision

1. Add a separate Rust DuckDB materializer, keyed by `(repository, worktree)`.
   It consumes the existing typed fact-version-change pages and does not read
   SQLite/Turso private tables or run any analyzed-language runtime.
2. Persist typed fact-version attributes under a uniqueness key of scope, fact
   identity, `valid_from_generation`, attribute name, and ordinal. A closed
   version replaces its prior open representation; reapplying the same page is
   idempotent. Store watermark and continuation cursor in the same DuckDB
   transaction as the corresponding row changes.
3. Track an in-progress generation separately from the last fully synchronized
   generation. While catch-up is incomplete, materialized query entry points
   fail closed instead of exposing a mixed-generation projection. Resume only
   the same pending generation/cursor after restart; require the next canonical
   generation sequence after a completed watermark.
4. Seed/repair only from a complete Parquet export that passes the full
   verifier. Keep the export's existing OS lock held from verification through
   import so partitions cannot change between checksum validation and DuckDB
   reading them. Build and count-check a staging table before replacing the
   live materialization in one DuckDB transaction. Any verification, read, or
   staging failure leaves the old materialization untouched. No pruning based
   on a partial source scan is permitted.
5. Keep orchestration separate: the Penelope integration's opt-in
   `analytics-duckdb` feature journals explicit sync/rebuild requests and
   retries idempotent materializer effects. Do not make DuckDB a transitive
   dependency of the engine or default Penelope integration. Materializer
   errors never alter or block accepted graph generations. StateChronicle
   verification remains an opt-in host concern, not a DuckDB dependency.
6. Internal transfer remains typed Rust/Arrow values. Parquet is the durable
   full-rebuild artifact; NDJSON/JSON text is neither the materialization
   store nor a service protocol.
7. Keep DuckDB schema bootstrap/upgrade explicit through `migrate`; `open`
   validates the schema version and required objects without applying a
   migration or repairing drift. This reuses Shardline's explicit
   migration/operator-startup split rather than relying on startup DDL.
8. Separate Penelope operation identity from request configuration: the
   operation key binds scope, target generation, database, and action/source;
   the request digest additionally binds execution options such as page size.
   A retry with changed options must collide with and fail validation against
   the existing operation record instead of creating a duplicate effect.

## Alternatives considered

- Replay all history or rebuild Parquet on every incremental update: rejected
  because work grows with total retained history rather than the changed page.
- Expose partially applied pages to queries: rejected because a query could
  combine old and new temporal closure state.
- Delete the live tables before a replacement export is fully verified:
  rejected because interrupted or dirty rebuilds must preserve the last good
  projection, matching Shardline's conservative reconciliation rule.
- Synchronously dual-write DuckDB during graph publication: rejected by
  ADR-0098's downstream failure-isolation boundary.

## Consequences

- Incremental writes are delta-proportional, retryable, and checkpointed with
  their source cursor; readers can distinguish a current materialization from
  one that is catching up.
- Rebuilds require staging space and a final transactional replacement.
- A pending generation temporarily makes the materialized query surface
  unavailable; canonical graph queries remain available throughout.
- The materializer's public API must remain in the optional analytics adapter,
  not core, public graph DTOs, runtime protocol, or canonical store ports.

## Verification

- Baseline rebuild verifies all export partitions and preserves existing data
  on corrupt, incomplete, or failed staging input.
- Insert/update/delete and retry fixtures match a full verified export,
  including temporal interval closures.
- A forced interruption between pages leaves an incomplete watermark, rejects
  materialized queries, and resumes from the persisted cursor after reopen.
- Rows and cursor/watermark roll back together on injected DuckDB transaction
  failure; completed generations enforce contiguous canonical sequences.
- Penelope request-digest tests prove that synchronization page size changes
  request configuration without creating a second logical operation.
- Core/store/query publication remains independent, and no NDJSON serialization
  or analyzed-language runtime is used by the materializer.
