# ADR 0239: Load retained graph projections through the store history port

Status: accepted

## Decision

Add `GenerationGraph::load_retained(store, generation)`, delegating to the
existing `GraphStore::historical_snapshot` and sharing projection construction
with current loading. Keep the existing `load` contract unchanged.
Historical cycle analysis needs a complete selected projection; do not make it
depend on current-only node/edge APIs or introduce another history reconstruction.

## Limits

This is output-sized graph materialization. The SCC kernel is O(V+E), with
additional ordered-map construction, endpoint checking, and result sorting. It does not replace
the indexed point/paged historical query paths or claim O(1) historical analysis.
Durable adapters retain their existing verified historical snapshot behavior.

## Verification

Full `cargo make ci` passes in 168.69 seconds, including strict workspace Clippy,
architecture/dependency checks, all four graph tests, and 16 backend conformance
scenarios. The focused projection fixture verifies historical cycle preservation
after a later edge removal and rejects an unavailable generation. The store
conformance suite independently verifies retained snapshots across durable
backends/restarts; it is not a cycle-specific cross-backend quality fixture.
