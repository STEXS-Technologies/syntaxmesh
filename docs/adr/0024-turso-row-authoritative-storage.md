# ADR-0024: Make Turso rows authoritative, not a restored whole-graph snapshot

- Status: accepted
- Date: 2026-09-27

## Context

The v5 source of truth says the live graph must not require loading one giant
JSON document. Although Turso stores individual SQL rows, the adapter restored
all graph rows into `InMemoryGraphStore` on open, cloned that complete state for
every write, and rewrote all graph tables for every delta. This makes the
snapshot—not indexed SQL rows—the practical authority and scales work with the
entire graph rather than the changed partition.

## Decision

Make the Turso schema's typed rows authoritative. Open validates schema and
indexed projections without restoring every fact. Apply a `GraphDelta` inside
one writer transaction using its changed/removed fact IDs, validate
referential integrity in SQL, stream the canonical root over sorted rows, and
publish the new manifest in the same transaction. Point and adjacency reads
remain SQL-backed; explicitly full-result APIs may still return a `Vec`.
Schema v5 stores provenance references as indexed canonical columns and
backfills them from v4 payloads transactionally.

The in-memory store remains the deterministic reference backend. Startup
logical integrity is an explicit streaming scan, not an in-memory materialized
copy. Status/export full-generation APIs are allowed to materialize their
requested result, but ordinary indexing and point queries are not.

## Consequences

- Normal startup memory no longer grows with the number of graph facts.
- Delta persistence is proportional to the delta (plus root and integrity
  scans in this iteration); it no longer rewrites every fact row.
- Turso schema and migration tests must prove v4 backfill, failed-migration
  rollback, row/projection consistency, and differential parity with the
  reference stores.
- Streaming root validation is still O(graph size); a versioned incremental
  Merkle/root structure is a separate measured optimization, not a reason to
  retain a whole-graph heap snapshot.
- This decision defines only the current-state projection. It does not satisfy
  first-class historical graph requirements by itself; see
  [ADR-0026](0026-current-projection-and-first-class-history.md). Accepted
  deltas must also be retained atomically in the append-only history layer.
