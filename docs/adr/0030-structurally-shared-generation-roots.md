# ADR-0030: Resolve historical generations through structurally shared roots

- Status: accepted
- Date: 2026-09-27

## Context

ADR-0029 removed delta replay from durable `GraphAt`, but the interval
projection still examines old versions of frequently changed facts. The
synthetic high-churn benchmark confirms a cost increase as generations accrue.
The source of truth calls for deterministic deltas plus structural sharing and
explicitly rejects duplicate full semantic worlds per generation (§§277–278).
Historical depth needs an immutable generation root that selects one logical
world without replay or scanning its prior versions.

## Decision

- Maintain an immutable, content-addressed, structurally shared fact map. Its
  deterministic key is `(fact_kind, stable_fact_id)` and its value is the
  canonical encoded file, provenance, node, or edge payload.
- Store one small `(generation_sequence, generation_id, root_id)` mapping per
  accepted generation. Publication updates the current projection, temporal
  fact history, append-only delta history, and the new persistent root in one
  transaction.
- Use a deterministic balanced persistent search tree (initially a
  content-addressed treap with priorities derived from fact keys) so a
  generation lookup selects its root by the generation index, point reads are
  expected O(log F), and a complete `GraphAt` is O(F) in its output size,
  independent of intervening generation count H. Updates write only copied
  search paths, expected O(C log F) new tree records for C changed facts,
  rather than a full graph copy per generation.
- Preserve temporal intervals, observation/acceptance metadata, manifests,
  deltas, and StateChronicle verification as distinct concerns. The shared
  root is the structural state view; version history answers evolution and
  bitemporal questions. Checkpoints remain recovery, migration, and rebuild
  accelerators. DuckDB/Parquet remains the path for broad analytics.
- Migrate retained history by rebuilding roots deterministically from its
  explicit anchor and accepted deltas. If history is incomplete or corrupt,
  fail closed and retain the prior database; do not invent a root for unknown
  generations. Validate every rebuilt root against each generation manifest.
- Garbage collection of unreachable shared tree pages is deferred until a
  safe retention/compaction policy can preserve every promised historical
  generation. Storage growth is measured and reported in the meantime.
- Durable full-snapshot reads fetch the selected root's reachable pages in one
  recursive indexed query per database backend, then materialize the requested
  graph in memory. This avoids a database round-trip per shared page while
  preserving output-size-proportional work.
- Snapshot facts are emitted in stable-ID order within each fact family before
  manifest-root verification. Tree traversal order is an implementation detail
  and must not affect canonical generation identity.

These costs describe the logical index, not strict wall-clock O(1). A full
snapshot always costs at least its output size, and search-tree point reads are
expected logarithmic in fact count. The key property is that selecting a
historical root and reading its state do not grow with H.

## Consequences

The explicit SQLite/Turso integrity commands validate retained checkpoints
against their generation-history manifests and persistent historical roots,
and report missing or out-of-cadence checkpoints. This adopts the useful
operator-facing `fsck` pattern from Shardline: check rebuildable artifacts
against canonical state and tie findings to durable locations. Validation stays
out of open and ordinary historical reads; persistent roots remain the query
path.

- SQLite and Turso need a versioned schema migration that backfills one
  structurally shared root per retained generation and atomically maintains
  roots for new accepted generations.
- A shared database-independent tree implementation must be tested for
  deterministic roots, insertion/replacement/removal, corruption detection,
  structural sharing, and differential equality with replay snapshots.
- Benchmark gates must vary H while holding F and result size fixed, and vary
  C while measuring bytes/pages written. They must include hot repeated edits,
  additions/removals, reopen, and older generation reads.
- Root pages are internal storage details; no new public DTO or SQL sequence
  leaks into the query API.
