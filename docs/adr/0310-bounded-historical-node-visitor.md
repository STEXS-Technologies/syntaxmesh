# ADR-0310: Reuse a visitor-style port for bounded historical node scans

Status: reference port implemented; optimized adapter and Query integration pending.

Reuse Shardline's visitor-style store APIs, adapted to SyntaxMesh's object-safe
GraphStore and fixed StoreError contract. Add `visit_historical_nodes` accepting
an exact generation, positive total node budget and a synchronous visitor.
Return the visited count only after complete traversal; exceeding the budget
returns InvalidPageLimit. Visitor errors stop immediately. Earlier callbacks
may have run before any later error, so callers must not publish partial results.

Nodes arrive in strictly increasing stable-ID order. The reference implementation
uses existing pages capped at 1,000 nodes, validating counts, progress and total
budget. It does not claim bounded physical memory for snapshot-backed adapters.
Keep the method object-safe, database-free and runtime-neutral; introduce no
host or transport types, workflow dependency, storage migration or new runtime.

The intended Turso override will validate authoritative history once within one
read transaction, then reuse the existing persistent walker, prepared SQL page
reader and bounded validated-page cache. Each separate operation still validates
again. Transaction cleanup and snapshot isolation must be tested before Query
uses the override. The initial reference port alone does not fix cold latency.

The existing cross-backend/restart node-page scenario passes with visitor
result equality, zero/exhausted budgets and immediate callback-error stopping
(9.85 seconds). All 34 Store library tests, strict all-target/all-feature Store
and Turso Clippy, formatting and architecture dependency checks pass. An added
unknown-generation scan check is pending its rerun. Full contributor CI,
optimized transaction-scoped Turso traversal, Query integration and latency
evidence remain open. No production retrieval path uses this port yet.

The unknown-generation check subsequently passes in the same cross-backend/
restart scenario (9.73 seconds). Full contributor CI is running for this port
change before the optimized adapter is introduced.

Full contributor CI subsequently completes successfully in 214.66 seconds with
two jobs, including all 17 backend scenarios (44.16 seconds), all-feature
workspace tests, strict workspace Clippy, mixed instrumentation checks,
extension/migration gates, dependency policy and documentation. Existing allowed
dependency-policy warnings remain. The reference visitor port is verified;
transaction-scoped Turso traversal and Query adoption remain unimplemented.
