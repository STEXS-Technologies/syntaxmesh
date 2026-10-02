# ADR 0238: Cycle analysis over generation-tagged projections

Status: accepted

## Decision

Add `GenerationGraph::cyclic_components(relation)` using crates.io petgraph's
iterative Kosaraju SCC implementation. Reuse the loaded generation projection;
do not implement a competing SCC algorithm. An optional exact relation filter
selects edges before analysis. Return only cyclic SCCs (multiple nodes or a
self-loop), sorting members and components by stable node identity.
Reject dangling selected edge endpoints as store integrity errors.

## Limits

The SCC algorithm is O(V+E); ordered-map endpoint checks and deterministic
sorting add logarithmic overhead to the wrapper. It allocates another adjacency
representation. It is not constant-time, a bounded query, or a complete list of
individual cycles. The result inherits extraction/resolution quality. This adds
the graph capability, not CLI/HTTP integration or architecture-rule evaluation.

## Verification

Full CI for the initial cycle component addition passes in 163.17 seconds,
including workspace Clippy and dependency/license/advisory checks for petgraph.
Focused graph tests cover directed components, self-loops, exact relation
selection, deterministic output, and dangling selected endpoints. The later
retained-loading addition is tracked separately by ADR 0239.
