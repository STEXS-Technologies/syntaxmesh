# ADR-0105: Carry canonical generation sequence in analytical history

- Status: accepted
- Date: 2026-09-29

## Context

Generation IDs are opaque content-derived identities, not clocks or sortable
positions. `GraphStore::generation_sequence` already exposes the canonical
one-based durable order, and SQLite/Turso resolve it from the retained
generation index. The fact-history Arrow/Parquet schema currently omits that
order. As a result, analytical consumers cannot correctly sort graph history or
maintain sequence watermarks for incremental synchronization.

DuckDB incremental synchronization needs exact accepted-generation order to
distinguish newly valid versions from versions whose validity interval closed.
Acceptance timestamps are not a substitute: they may be absent, equal, or
subject to wall-clock behavior. Generation IDs must remain unchanged.

## Decision

1. Extend the store's bounded `FactHistoryEntry` with the one-based canonical
   sequence for its `valid_from` generation and its visible `valid_until`
   sequence. Resolve these from the existing retained generation sequence
   index in durable adapters; the reference store derives them from its ordered
   history.
2. Have `FactHistoryBatchReader` pin and carry the `as_of_generation_sequence`
   for every emitted Arrow batch. Do not add timestamps as ordering keys.
3. Bump the fact-history Arrow schema version to 2 and append three `UInt64`
   fields: `as_of_generation_sequence`, `valid_from_generation_sequence`, and
   nullable `valid_until_generation_sequence`. Keep all existing field order
   and meanings intact; stable generation IDs remain present.
4. Keep the Parquet manifest format version at 1 and set its existing
   `schema_version` to 2 for new exports. A v1 export is rejected for v2
   analytics and must be re-exported to a new/empty destination; do not
   reinterpret old bytes or mutate the canonical store.
5. DuckDB orders and watermarks temporal analytics only by this sequence. Its
   sequence-aware APIs remain downstream and separate from canonical writes.

## Alternatives considered

- Sort by generation ID: rejected because IDs are opaque hashes.
- Sort by acceptance timestamp: rejected because wall-clock ordering is not
  guaranteed, and timestamps can be unavailable or equal.
- Walk parent links or load all generation history during each analytics
  query: rejected because it adds history-depth work and replays data the store
  already indexes.
- Add a separate generation lookup per exported fact: rejected because it
  would create query amplification. Durable fact-history page queries already
  read the indexed start/end sequences and now return those values directly.

## Consequences

- Fact-history exports can be ordered chronologically and can support
  generation-watermarked incremental analytical sync without changing graph
  identity or canonical temporal semantics.
- New exports use Arrow schema v2. Existing v1 datasets remain readable only by
  compatible v1 consumers and are not silently upgraded in place.
- No canonical database schema migration is needed; sequence columns already
  exist in SQLite/Turso temporal projections.

## Verification

- InMemory, File, SQLite, and Turso pages report identical one-based start/end
  sequences for the same pinned graph history.
- Arrow and Parquet preserve the sequence columns and schema-v2 metadata.
- A two-generation fixture verifies chronological order and interval closure.
- Existing v1 exports fail closed under the v2 verifier instead of being
  interpreted as sequence-aware datasets.
