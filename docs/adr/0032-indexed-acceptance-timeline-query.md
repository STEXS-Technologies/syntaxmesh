# ADR-0032: Expose a bounded indexed acceptance timeline

- Status: accepted
- Date: 2026-09-27

## Context

ADR-0031 established acceptance time as knowledge-time metadata, persisted
atomically with each accepted generation. The SQLite and Turso stores now have
ordered acceptance-time indexes. Without a query contract, that index cannot
yet serve a caller, while introducing `StateAtTime` would overstate the current
semantics: SyntaxMesh does not yet define clock rollback handling, modeled
calendar validity, or bitemporal evaluation.

## Decision

- Expose `accepted_generations_between(from_inclusive, until_exclusive, after,
  limit)` from the store/query boundary. It returns only known acceptance
  records in the half-open timestamp range, ordered by
  `(accepted_at, generation_id)`.
- Require a non-empty interval and positive result limit. The operation is a
  bounded timeline listing; it does not choose or reconstruct a graph state.
- Return a typed continuation cursor in the page/footer when more rows exist.
  The cursor contains the last `(accepted_at, generation_id)` pair, so paging
  across timestamp ties is deterministic and durable stores can seek forward
  in the composite index without scanning earlier rows.
- SQLite and Turso answer from the ordered acceptance-time index. InMemory and
  File are reference implementations and may scan retained generations.
- A versioned temporal interchange record carries the generation manifest and
  its typed acceptance time. Legacy generations with unknown acceptance time
  are omitted; timestamps are never fabricated.
- Do not add `StateAtTime`, `KnownAt`, or bitemporal state selection here. A
  later ADR must specify which generation is visible at a time cutoff,
  including non-monotonic injected clocks, transaction ordering, and modeled
  validity, before that query can be implemented.

## Alternatives considered

- **Treat acceptance time as graph validity and expose `StateAtTime`:**
  rejected because acceptance is knowledge time, not modeled system time, and
  clock ordering semantics are not yet specified.
- **Return every generation in the store:** rejected as the default calendar
  query because work/output would grow with all retained history. An explicit
  time window and caller limit bound the read.
- **Scan timestamps in durable backends:** rejected because the persisted
  ordered index exists specifically to make range selection independent of
  history depth.

## Consequences

- The acceptance-time index becomes an exercised query path rather than
  dormant storage metadata.
- Query cost is indexed range seek plus at most `limit` returned records;
  SQL engines provide logarithmic index seek, not a strict O(1) guarantee.
  This keeps fixed-size timeline pages independent of total history depth in
  durable stores; arbitrary full graph output remains output-sized.
- This is a timeline foundation, not complete temporal graph intelligence.
  Observation-time queries, edge/fact history DTOs, consequence lineage,
  bitemporal state evaluation, and hot/warm/cold planning remain separate
  work.
