# ADR-0034: Expose a bounded indexed observation timeline

- Status: accepted
- Date: 2026-09-27

## Context

ADR-0031 distinguishes producer-reported observation time from engine
acceptance time. ADR-0033 exposes both coordinates on an individual fact's
history, but callers still cannot find facts by event time. The SQLite/Turso
fact-version table already stores runtime-observation timestamps as ordered
big-endian bytes; its current index omits the version-start sequence needed
for a deterministic cursor when one fact has multiple versions at the same
observation timestamp.

## Decision

- Expose a bounded `observed_between(from_inclusive, until_exclusive, after,
  limit)` query over canonical fact versions with a known producer observation
  time. Results list evidence versions; they do not select graph state.
- Use the half-open interval `[from, until)`. Sort by
  `(observed_at, fact_kind, fact_id, valid_from_sequence)` where sequence is
  retained parent order for multiple versions of one fact. The typed cursor
  carries the starting generation ID instead of the internal sequence; never
  expose database sequence numbers.
- Return the typed fact payload, generation-validity bounds, original
  observation time, and that version's engine acceptance time. Unknown legacy
  acceptance times and absent observation times remain unknown; facts with no
  observation time are not fabricated or returned.
- SQLite/Turso range over the partial composite index
  `(observed_at, fact_kind, fact_id, valid_from_sequence)`; the generation
  cursor resolves to the internal sequence inside the query. Reference stores
  scan retained histories for conformance.
- V0 currently indexes runtime observations because that is the only
  producer-timestamped canonical fact family. Adding other event-time sources
  requires explicit producer semantics and metadata extraction, not reuse of
  engine acceptance time.
- Do not add `StateAtTime` or `KnownAt`. Observation time says when the
  producer says an event occurred; it does not say when a graph fact was valid
  or when SyntaxMesh knew it.

## Alternatives considered

- **Use acceptance time for event-time search:** rejected because delayed
  telemetry would be placed at import time rather than occurrence time.
- **Return `GraphAt` for the observed timestamp:** rejected because producer
  event time is neither generation validity nor a transaction ordering rule.
- **Unbounded scan and sort in durable stores:** rejected because an existing
  timestamp index can provide a bounded range seek; the index must include the
  deterministic version tie-breaker.

## Consequences

- A caller can page through runtime evidence by producer event time with work
  bounded by the index seek and page size rather than total retained history.
- This is a temporal evidence timeline, not full bitemporal graph evaluation,
  historical conclusion replay, or consequence lineage.
- SQLite/Turso schema migrations add the composite event-time index without
  changing canonical payloads or graph roots. Cross-backend tests cover ties,
  cursor continuation, range boundaries, restart, unknown timestamps, and
  high-history-depth fixed-size pages.
