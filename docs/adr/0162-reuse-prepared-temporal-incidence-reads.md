# ADR-0162: Reuse prepared temporal incidence reads

- Status: accepted
- Date: 2026-09-30

Reuse the Turso adapter's existing prepared-statement pattern for repeated
point reads within one historical incidence page. Prepare the tree-page lookup
once and reuse it for outer and inner tree traversal; prepare edge hydration
once for the selected IDs. Share the existing row decoder rather than introducing
a second serialization path. Keep statements query-local, validity predicates,
ordering, error checks, and instrumentation statement counts unchanged.

This removes repeated SQL preparation, not SQL point reads. No schema, public
contract, cross-query cache, or complexity guarantee changes. Validate with the
multi-page cross-backend deletion/restart fixture and strict Clippy; compare its
opt-in query timings with the recorded baseline before claiming a speedup.

## Verification

All 37 Turso unit tests, strict Clippy, and the multi-page cross-backend fixture
pass. The isolated fixture completes in 80.62 seconds versus the preceding
89.27-second run. Its first dense Turso historical packs take 4.07/4.52 seconds
at capacities one/two, versus 5.92/6.32 seconds previously. Later measurements
overlapped full CI, so these are indicative single-run debug timings, not a
controlled benchmark, release latency, or complexity claim. The fixture retains
byte-for-byte pack parity, exact budgeting, edge deletion, and durable restart
checks. Degree-proportional SQL hydration and tree-page reads remain.
The complete `cargo make ci` gate passes, including architecture and migration
checks, workspace strict Clippy, all default tests, and documentation builds.
