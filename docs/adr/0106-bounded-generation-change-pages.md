# ADR-0106: Bounded, generation-pinned pages for accepted changes

- Status: accepted
- Date: 2026-09-29

## Context

DuckDB currently reads only complete verified Parquet snapshots. Its
materialized incremental synchronization is still open. The store has
`changes_between(from, to)`, but that operation returns a `Vec` for the full
range. SQLite and Turso use the generation-history sequence key for the range
lookup, yet they decode and retain every delta before returning. That is not a
bounded resumable feed for a large analytical catch-up.

Canonical generation order is already durable and one-based. In SQLite and
Turso, `syntaxmesh_generation_history.sequence` is the integer primary key;
InMemory/File provide the reference history scan. Parquet already supplies a
complete verified rebuild source, while Penelope journals/retries explicit
downstream exports. Shardline's rebuild code reinforces that rebuild is a
reconciliation path over immutable records, not an excuse to make normal
incremental work rescan all prior versions. Its storage-specific rebuild
implementation is not copied.

The feed must remain Rust-typed and downstream. NDJSON is not an internal
handoff, and DuckDB must not join canonical publication or become a second
source of truth.

## Decision

1. Add `GraphStore::changes_between_page(from, to, after, limit)`, with a
   version-independent cursor bound to both range endpoints. Each returned
   `GenerationChange` carries its canonical one-based sequence. The next cursor
   binds the same endpoints and the last accepted sequence/generation.
2. Enforce forward, retained, contiguous ranges and reject mismatched or
   out-of-range cursors. Pages are bounded by
   `MAX_GENERATION_CHANGE_PAGE_SIZE`; callers persist progress only after
   their downstream transaction has committed.
3. In SQLite/Turso, resolve endpoint and cursor sequences by generation key,
   then fetch only `sequence > after AND sequence <= to ORDER BY sequence
   LIMIT limit+1` from the existing primary key. Do not add a migration or a
   duplicate sequence index. InMemory/File keep a clearly documented
   history-scan reference implementation.
4. Keep `changes_between` for explicit full-range callers. It returns the same
   generation/delta content, now including sequence, and is not advertised as
   bounded.
5. The next DuckDB sync slice may consume these pages and record its
   generation watermark in the same DuckDB transaction as materialized fact
   changes. Initial construction and repair continue to use complete,
   verified Parquet exports. Penelope remains responsible for the durable
   workflow request/recovery boundary; no synchronous OLTP/DuckDB write is
   introduced by this store API.

## Alternatives considered

- Call `changes_between` and slice the returned vector: rejected because it
  still loads the entire interval before a consumer can checkpoint.
- Re-export and rescan all historical Parquet partitions on every sync:
  rejected as an O(total history) substitute for incremental catch-up.
- Read adapter-private SQLite/Turso tables from DuckDB: rejected because it
  couples analytics to backend schemas and bypasses the runtime-neutral store
  contract.
- Add a new sequence index: rejected because the generation-history sequence
  is already the primary key and directly supports the bounded range query.
- Use NDJSON as a change journal or service protocol: rejected; typed Rust pages
  are the internal API and NDJSON remains explicit external export only.

## Consequences

- Consumers can catch up through immutable accepted changes with bounded
  per-page memory and durable sequence watermarks.
- The durable adapters add no schema migration. The reference backend remains
  intentionally O(history) and is used for differential correctness, not the
  production complexity claim.
- This contract alone does not implement materialized DuckDB synchronization,
  prove end-to-end O(delta) work, or guarantee that every single generation
  delta is small. Those require the transactional DuckDB effect, recovery,
  query-plan, and changed-fact-count fixtures.

## Verification

- Cross-backend pages concatenate to exactly the same ordered changes as
  `changes_between`, including canonical sequence numbers.
- Cursor binding, range validation, empty/exhausted pages, and continuation
  across later publication are covered.
- SQLite/Turso query-plan tests confirm sequence-key range seeks with `LIMIT`,
  not a full generation-history scan.
- Existing full-range history, lineage, publication, and analytics export
  tests remain unchanged in meaning.
