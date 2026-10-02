# ADR-0090: Measure historical-adjacency index page reads in benchmarks

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0085/0086 require evidence that a fixed historical-neighbor page does not
replay retained generations or scan unrelated graph facts. The existing
Shardline-style `benchmark-temporal` harness measured elapsed time and checked
result shape, but elapsed time alone cannot establish what storage work a page
performed. SyntaxMesh already has a `benchmark-instrumentation` feature used to
collect write/publication stage metrics without affecting default builds.

## Decision

1. Extend that opt-in feature with per-page counts for SQL read-statement calls,
   persistent incidence-tree page-cache lookup attempts (including
   misses/retries), pages loaded from SQLite/Turso, historical edge payload rows fetched, and
   complete reference snapshots materialized by InMemory/File paths.
2. Have `benchmark-temporal` validate and print these counters beside its
   latency and result-shape evidence. Durable pages must fetch only the
   requested edge payloads; reference stores explicitly report snapshot
   materialization instead of claiming an indexed read.
3. Keep counters behind `benchmark-instrumentation`; normal builds, CLI/API
   records, stored schemas, and runtime behavior do not gain instrumentation
   overhead or diagnostic fields.
4. Count each read at its single-statement adapter call site and test the
   expected relationship between statement count, index-page loads, and edge
   payload rows. Treat all counts as fixture-specific storage-operation
   evidence, not tree-comparison counts, an asymptotic proof, or an SLA.

## Consequences

- A stable incidence-page load count across retained-generation depths directly
  checks that the tested durable page path does not replay history.
- Tree-page visits and loads may rise with unrelated index size as search paths
  deepen; this metric does not imply strict O(1) behavior in graph size.
- Broader graph distributions and production workload evidence remain separate
  follow-up work.

## Verification

Run `cargo make benchmark-temporal`; each backend's fixed 10-edge page fixture
must validate output ordering, continuation, and its expected read-counter
shape. SQL statements are counted at the adapter's individual single-statement
call sites; for the populated fixtures, the expected count is
`2 + loaded_tree_pages + fetched_edge_rows`. The run reported 30 statements
(18 pages, 10 rows) at every tested history depth, and 36/41 statements
(24/29 pages, 10 rows) at 100/1,000/5,000 graph nodes. InMemory reported zero
SQL statements and one reference snapshot. This is fixture-level call-site
accounting, not database-engine tracing. Run the default workspace build
without the feature to verify the instrumentation remains opt-in, and run the
full CI gate with all features.
