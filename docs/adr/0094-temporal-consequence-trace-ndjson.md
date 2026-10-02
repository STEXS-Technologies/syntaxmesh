# ADR-0094: Stream temporal consequence traces as versioned NDJSON

Status: Accepted

## Context

[ADR-0093](0093-generation-range-consequence-traces.md) defines bounded,
chronological, multi-generation consequence traces in the runtime-neutral
query layer. The result is currently available only as Rust query structs.
Consumers need the same versioned line-oriented interchange used by existing
historical and consequence-neighborhood queries.

## Decision

- NDJSON is an explicit query/export representation only. It is not a durable
  storage format and is not used as an internal service-to-service protocol;
  internal calls pass typed Rust values through the query and store APIs.
- Append temporal-trace header, state, hop, and footer variants to
  `TemporalRecord`, with an independent schema version 1. Do not bump the
  existing temporal-record schema used by unrelated streams.
- The header binds the stream to the exact origin and inclusive generation
  range. State records preserve the distinct `(endpoint, generation, depth)`
  DAG vertices. Hop records preserve source/target direction, per-hop arrival
  generations, and the complete consequence edge version including its
  exclusive retraction generation, evidence, derivation, and provenance.
- The footer reports emitted state and hop counts, scanned incidences, and
  explicit truncation. A byte cap is applied to complete serialized lines;
  records are never cut mid-line. The query/store path remains responsible for
  bounded selection, and output truncation never changes canonical state.
- Keep this contract in the runtime-neutral API-model and query crates. CLI
  access is provided by `consequence-trace` and `consequence-trace-turso`,
  taking a store, inclusive `from`/`until` generations, an `event <id>` or
  `fact <kind> <id> <valid-from>` origin, and explicit hop, endpoint, edge,
  incidence, and serialized-byte caps. These commands call the same query
  method and do not duplicate traversal or serialization semantics.
- This is historical evidence, not a causal assertion. Do not union edges
  across time, replay each generation, or infer missing states or causes.

## Consequences

The trace can be consumed incrementally without Rust types while retaining its
temporal DAG and provenance. Adding enum variants is append-only; old record
variants and the existing fixed-generation neighborhood stream remain
unchanged. The trace schema can evolve independently from other temporal
record families.

## Verification

- Round-trip every new record variant through one JSON line.
- Compare temporal-trace NDJSON from the query layer with the typed trace,
  including chronology, retraction bounds, and explicit byte truncation.
- Exercise both file and Turso CLI commands over a published multi-generation
  fixture and verify their emitted temporal records.
- Keep tests on the shared query/store APIs and runtime-neutral dependency
  boundaries.
