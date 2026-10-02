# ADR-0097: Measure composed temporal consequence trace work

- Status: Accepted
- Date: 2026-09-29

## Context

ADR-0096 measures one generation-range endpoint page, but a temporal trace
composes generation lookups, endpoint pages, and optionally evidence reads.
Page-level counts alone do not show the work performed by a complete query.

## Decision

Add opt-in, benchmark-only read counters to the typed
`Query::consequence_trace` result and benchmark a deterministic two-hop trace
across two endpoint pages at retained-history depths 64, 256, and 1,024 on
the reference, SQLite, and Turso stores. Count endpoint pages,
generation-sequence lookup calls, provenance lookup calls/IDs, and the store
counters already reported by each endpoint page. The fixture must assert the
same logical trace across backends.

These counters are diagnostic evidence for this fixture, not an O(1) claim,
not production telemetry, and not a claim about arbitrary graph topology.
They are excluded unless `benchmark-instrumentation` is enabled.

## Verification

`cargo make benchmark-temporal` completed on the synthetic fixture. Every
backend returned two hops, three endpoint states, and scanned three
incidences. At depths 64/256/1,024, InMemory made 2 endpoint pages and
materialized/scanned 128/512/2,048 generation entries and consequence-history
entries, examining four mutation records. SQLite and Turso each made 2 range
pages, whose page counters now report 2 SQL reads total (one statement per
page) and 3 returned rows. A query-local generation-sequence cache now
deduplicates repeated generation IDs: the fixture performs 2 sequence lookups
instead of 5. With no provenance filter in this fixture, durable stores
therefore perform 4 counted SQL calls total (2 sequence lookups + 2 range-page
queries). Median trace times were 147.6/148.1/143.1 µs (SQLite),
280.9/278.4/279.8 µs (Turso), and 62.7/231.2/925.4 µs (InMemory). One
synthetic chain and one host do not establish general topology-independent
complexity or a performance guarantee.

## Consequences

The benchmark measures composed query work without changing the normal query
API build or introducing serialization into the query path. NDJSON remains an
explicit export representation; this instrumentation observes the typed query
and store calls directly.

## Alternatives considered

- Infer total work from endpoint-page measurements: rejected because it omits
  query-level lookups and multiplicity of endpoint pages.
- Add always-on production counters: rejected because measurement should not
  add cost or public fields to ordinary builds.
