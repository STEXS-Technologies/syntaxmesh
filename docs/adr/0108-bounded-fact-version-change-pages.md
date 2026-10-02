# ADR-0108: Bounded pages of fact versions changed at one generation

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0107 adds an identity-scoped exact lookup for fact versions opened or
closed by one accepted generation. A materializer still needs to enumerate all
such versions. Enumerating a generation delta first is not bounded by fact
count, and following it with one store call per identity makes a large
generation an unbounded intermediate plus an N+1 read pattern.

Analytics synchronization must be downstream of accepted graph publication,
delta-proportional, resumable, and rebuildable from verified Parquet. Penelope
owns its durable workflow/retry boundary. Core and the store port remain
database/workflow/transport independent; NDJSON remains external export only.

## Decision

1. Add `GraphStore::fact_version_changes_at_page(generation, after, limit)`.
   It returns only canonical fact versions whose valid-from or exclusive
   valid-until sequence equals that accepted generation. Updates can return
   both the closed and opened version. Ordering is stable by `(FactRef,
   valid_from_sequence)`.
2. Bind each cursor to the exact generation and last returned fact-version
   key. Reject a cursor for another generation, invalid sequence, or a limit
   outside `MAX_FACT_VERSION_CHANGE_PAGE_SIZE`. Reference stores may scan
   their retained typed fact history; durable backends must seek temporal
   indexes.
3. SQLite/Turso add registered indexes led by
   `(valid_from_sequence, fact_kind, fact_id)` and
   `(valid_until_sequence, fact_kind, fact_id)`. The two exact-sequence seeks
   exclude same-generation duplicate closure rows. Each keyset-filtered branch
   is ordered and limited to `limit + 1` candidates before the bounded merge;
   the merged page returns at most `limit` versions. Indexes are additive and
   migration status remains read-only.
4. A downstream analytics consumer can checkpoint the cursor only in the
   same DuckDB transaction as the corresponding typed fact-version rows. A
   verified Parquet rebuild remains the recovery/repair path; Penelope journals
   the workflow request, not individual SQL effects.
5. No NDJSON, JSON-text, or service-protocol handoff is added. This is a Rust
   store-port contract and creates no analytics coupling in core or the live
   graph publication path.

## Alternatives considered

- Materialize the full `GraphDelta` before paging fact versions: rejected
  because memory and work depend on the largest transition, not page size.
- Call the identity-scoped ADR-0107 API once per changed fact: rejected as
  N+1 reads and requiring an unbounded identity list first.
- Scan all retained history on SQLite/Turso: rejected because each incremental
  step would grow with old history.
- Read adapter-private temporal tables from DuckDB: rejected because it
  bypasses the store contract and duplicates migration assumptions.

## Consequences

- Durable consumers can enumerate a generation's changed versions with
  output-bounded pages and indexed exact-sequence keyset lookups. Each page
  materializes at most `2 * (limit + 1)` pre-merge candidates.
- The reference implementation remains intentionally history-scan based for
  differential conformance; only durable-backend query plans establish the
  indexed lookup path.
- This API enables but does not itself implement the DuckDB transaction,
  watermark, Penelope recovery, full rebuild replacement, or temporal
  analytical query surface.

## Verification

- Insert, update, delete, unchanged, same-generation close/open, continuation,
  and cursor-binding behavior conform across InMemory, File, SQLite, and Turso.
- SQLite/Turso plans prove both generation-leading temporal indexes are used.
- Registered forward migrations from the previous schema versions and
  full-workspace CI pass.
