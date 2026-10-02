# ADR-0096: Measure consequence-range history-depth work

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0093 defines consequence traces from bounded endpoint/range pages. The
durable range-page implementations use temporal endpoint indexes, while the
reference implementation reconstructs intervals from retained generation
history. Latency alone does not show whether this lower-level query work grows
with retained history depth. SyntaxMesh already has opt-in
`benchmark-instrumentation` counters and the `benchmark-temporal` harness for
historical-neighbor reads (ADR-0090); extend those rather than introducing a
second benchmark framework or putting diagnostics in normal runtime builds.

## Decision

- Under `benchmark-instrumentation` only, attach consequence-range read counts
  to `ConsequenceRangePage`. Durable adapters count their endpoint-range SQL
  query, which resolves both generation bounds and reads the indexed page in
  one statement. The reference path counts
  materialized generation/consequence history entries, consequence entries
  traversed through the requested upper bound, and add/retract mutations
  examined. All backends count matching consequence rows returned.
- Extend `benchmark-temporal` with one assertion added at the first generation
  and retained across a fixed one-edge result range. Hold result size and
  mutations constant while testing the existing 64/256/1,024 history depths.
  Validate the result shape and counter values for InMemory, SQLite, and Turso.
- Treat these as fixture-level counts for one endpoint-range page. They do not
  count all pages in a multi-hop `Query::consequence_trace`, database-internal
  B-tree comparisons, or establish asymptotic complexity, a production latency
  guarantee, or an SLA.

## Consequences

The test distinguishes reference-history reconstruction from durable indexed
reads using observable adapter operations. Feature gating keeps metrics out of
normal builds, query records, persistence formats, and runtime behavior. Future
whole-trace instrumentation may aggregate page metrics if it is needed to
measure topology-dependent traversal work.

## Verification

`cargo make benchmark-temporal` measured one returned consequence row at each
depth. InMemory materialized and scanned 64/256/1,024 generation entries and
consequence entries, examining one consequence mutation. The original durable
path used three SQL reads per page (two bound lookups plus the indexed page).
The current query folds the bound lookups into the indexed SQL statement;
SQLite and Turso each report one page-level SQL read at every tested depth.
On the 2026-09-29 rerun, median page latency was 16.7/64.7/267.0 µs for
InMemory, 65.8/65.3/66.3 µs for SQLite, and 123.4/127.7/122.8 µs for Turso.
This confirms depth-independent page-level statement counts on this synthetic
fixture, not total trace statement counts, a general latency claim, or an SLA.
