# ADR-0099: Snapshot-pinned pages over canonical fact history

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0098 establishes a typed, downstream analytics feed. `GraphStore` currently
exposes full generation-history vectors and point history for one `FactRef`,
but no bounded global scan over temporal fact versions. An exporter that loops
over generations and applies every delta would replay the history to discover
facts and defeat the indexed temporal history already maintained by SQLite and
Turso.

Both durable schemas store fact versions in `(fact_kind, fact_id,
valid_from_sequence)` order, with generation metadata in the adjacent history
table. This is the established Shardline-style approach to durable migrations
and indexed queries: make the ordering explicit, page by a bound cursor, and
keep one query contract across the durable adapters rather than exposing SQL
layouts to consumers.

## Decision

1. Add a bounded `fact_history_page` read to `GraphStore`, ordered by
   `FactRef` identity then `valid_from` generation.
2. Pin every page and its cursor to one retained `as_of_generation`. Newer
   publications cannot enter the export; versions whose end lies after the
   pinned generation are reported open at that snapshot.
3. Cursor identity carries the pinned generation and the last fact/version
   key. Reject a cursor used with a different snapshot; `limit == 0` returns
   an empty page without advancing.
4. SQLite and Turso use the existing composite fact-history index and bounded
   SQL reads. InMemory/File provide a correctness reference implementation;
   their scan cost is not represented as equivalent performance.
5. Return typed `FactRef` plus `FactHistoryVersion` values. Serialization and
   Arrow/Parquet conversion belong downstream in the analytics adapter.
6. Do not add schema migrations or change publication transactions; this is a
   read-only query contract over existing retained history.

## Alternatives considered

- Replay every `GraphDelta` to discover and emit versions: rejected for durable
  stores because cost grows with retained history and bypasses existing indexes.
- Export all versions for one fact repeatedly: rejected because it requires an
  external identity-discovery phase and unbounded point-result vectors.
- Expose SQL rows/sequence numbers publicly: rejected; those are backend
  implementation details, not portable fact identities.
- Add a second analytics copy to canonical publication: rejected by ADR-0098;
  the page API supports an explicit downstream export without dual writes.

## Verification

- Pages are globally ordered, bounded, complete under cursor continuation, and
  cursor-bound to the pinned generation across InMemory, File, SQLite, and
  Turso.
- A fixture with later publications proves snapshot pinning and open-ended
  validity at the earlier `as_of_generation`.
- SQLite/Turso query plans use the existing composite history index.
- No schema migration is introduced and reads do not mutate canonical state.
- Implemented 2026-09-29 in the shared store port. A four-backend conformance
  fixture exercises bounded ordering, cursor continuation after a later
  publication, as-of validity clipping, and snapshot-bound cursor rejection.
  SQLite and Turso query-plan tests confirm the composite identity/sequence
  history index is used; no migration was added.
