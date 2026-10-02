# ADR-0027: Layer temporal graph queries instead of replaying history on reads

- Status: accepted
- Date: 2026-09-27

## Context

ADR-0026 preserves accepted generation deltas and currently reconstructs a
requested historical snapshot by replaying those deltas. This is correct for
small fixtures, but its cost grows with the distance from the history anchor
for every request. That conflicts with the source-of-truth's latency classes
and requirements for precomputed lineage indexes, checkpoints, materialized
summaries, and bounded historical queries (§§237, 277–279, 296–297). SyntaxMesh
must provide temporal depth without making every read traverse the lifetime
graph or reload every historical generation.

## Decision

Separate history into four layers with distinct read/write roles:

1. **Current projection:** indexed current facts for ordinary hot reads.
2. **Append-only change history:** accepted generation manifests and complete
   deltas, atomically committed with the current projection. This is the
   auditable source for rebuilding derived history indexes, not the default
   query path.
3. **Temporal fact projection:** versioned fact payloads with stable identity,
   valid-generation interval, observed/knowledge time, and accepted generation.
   Index by fact identity and interval, plus query dimensions such as owner,
   relation, provenance, and producer. Point-in-time and recent-history reads
   use these indexes and return work bounded by index lookup plus result size,
   not total history depth.
4. **Checkpoints and analytical projections:** periodic full-state checkpoints
   bound cold reconstruction and support index rebuild. Stream long-range and
   aggregate analysis through DuckDB/Parquet or equivalent analytical
   projections; do not construct a monolithic lifetime graph in memory.

“O(1) over history depth” means common indexed point lookups do not replay or
scan every intervening generation. A B-tree lookup is technically O(log n),
and any query returning k facts costs at least O(k); SyntaxMesh must not promise
strict O(1) for arbitrary graph output. Recent diffs and last-change/origin
lookups are HOT/WARM indexed queries. Unbounded retrospective analysis is COLD,
streamed, scoped, and explicitly bounded.

The trait's default `GraphStore::historical_snapshot` remains a reference path
for in-memory and file stores. Turso and SQLite maintain fact-version
intervals, use an identity/version index for historical node reads, and write
full graph checkpoints at generation 1 and every 64 accepted generations.
Initially their full-snapshot path replayed up to 63 deltas from the nearest
checkpoint; ADR-0029 replaced replay with interval reads. ADR-0030 now serves
full snapshots by structurally shared generation roots, so neither prior
version scans nor deltas are on the ordinary durable read path. Checkpoints
remain for recovery, migration, and index rebuild. The cadence is an initial
engineering choice, not a benchmarked product guarantee.

This is storage/query foundation, not complete temporal intelligence: the
initial public `History(node)`/`ChangedBetween(a,b)`/`GraphAt(generation)`
operations now exist, and acceptance time is captured through an injected
clock and atomically persisted for new generations. ADR-0033 exposes typed
history for canonical files, provenance, nodes, and edges with separate
optional observation/acceptance times. A bounded producer-observation timeline
is now exposed by ADR-0034, but calendar-time state selection, semantic lineage
indexes, hot/warm/cold planning, retention policy, and representative scale/cost
benchmarks are still missing. Full bitemporal state selection is future work;
generation-sequence numbers are not wall-clock times. ADR-0031 decides the
three-axis logical contract and historical interpretation modes.

## Consequences

- Accepted deltas remain necessary for audit, deterministic rebuild, and
  StateChronicle verification, but are not read-time indexes.
- Turso/SQLite maintain temporal fact versions and checkpoints atomically with
  published deltas. Schema migrations backfill checkpoint indexes from
  retained history where possible; a pre-history database still starts at an
  explicit current-state anchor.
- Historical fixtures cover point-in-time state, fact edits/removals, typed
  history for canonical fact families, root reads beyond one checkpoint
  boundary, and bounded observation-time pages. Representative query-cost
  benchmarks remain required before product readiness.
- Query APIs need explicit generation/calendar ranges and result bounds; broad
  semantic analytics are a separate streamed analytical workload.
- Checkpoint cadence and retention are benchmark-driven. Compaction must retain
  enough canonical evidence to reproduce published history.
