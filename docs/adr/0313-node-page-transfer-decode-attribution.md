# ADR-0313: Separate node-page transfer from envelope decoding

Status: implemented; contributor CI and whole-Sim attribution passed.

Reuse the private feature/environment-gated NodeReadProfile convention to split
ADR-0312's SQL-page region into query/row/BLOB-transfer time and Bincode page
envelope decoding. Retain the enclosing SQL-page metric and page counts for
comparison; new subregions are nested and must not be added to that total.
Apply only to the transaction-scoped node visitor. Do not change normal-build
read behavior, logs, clocks, storage, cache budgets or public contracts.

The instrumented helper must preserve the existing prepared query, row loop,
BLOB-only handling, Bincode decoding, Vec accumulation and missing-page checks.
Verify disabled timers/counters, non-feature compilation, strict Clippy and scan
cleanup/corruption tests before representative profiling. Attribution alone is
not an optimization or latency acceptance.

49 regular Turso unit tests pass with the instrumented helper, including scan
cleanup/corruption and enabled/disabled profiler coverage (one timing diagnostic
ignored). Rename query/transfer/decode timer bindings after strict Clippy rejects
unrelated shadowing; no allowance is added. Final strict Clippy and normal-build
checks are running; contributor CI and representative attribution are pending.

Strict all-target/all-feature Turso Clippy and the no-default-feature check
subsequently pass. Full contributor CI and whole-Sim attribution are running
with the existing private disk-backed scratch parent and workload policy.

Full contributor CI completes successfully in 199.94 seconds with two build
jobs, including all 17 backend scenarios (41.97 seconds), strict workspace
Clippy, mixed feature checks, architecture/extension/migration/dependency gates
and documentation. Whole-Sim attribution remains live on the unchanged
2,572-file input fingerprint; no new performance result is available yet.

Whole-Sim `context-20261002T130658.212859Z-uncommitted` completes in 483.77
seconds with the same input fingerprint, 87,486 nodes, 567,068 postings and
21,071,384 charged bytes. All five required 8,192-token targets remain; 3/5
remain at 2,048 tokens. Its 90,556 page loads spend 8.232209 seconds in the
enclosing SQL-page region: 6.417444 seconds SQL/row/BLOB transfer and 1.732689
seconds Bincode envelope decoding. These subregions are nested, not additive
with the enclosing total. Index build is 12.830644 seconds; cold context is
13.324809 seconds; retained 8,192-token queries take 0.713212–0.956699 seconds.

Transfer dominates envelope decoding in this debug sample. A bounded known-ID
frontier using existing prepared batch-query patterns is the next candidate,
with explicit snapshot/cache/budget/corruption rules before implementation.
No new speedup is attributed to diagnostics or the modest run-to-run difference
from ADR-0312. General relevance, repeatable cold latency and default promotion
remain open.
