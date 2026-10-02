# ADR-0311: Traverse historical Turso nodes in one validated read transaction

Status: implemented; full contributor CI passed.

Implement ADR-0310's visitor port using the existing Turso deferred transaction,
persistent range walker, prepared page reader and bounded validated-page cache.
Read authoritative history/schema and root once inside that transaction, then
visit strictly increasing node IDs without materializing a full node vector.
Validate each loaded page and fact identity as before. Stop immediately on
visitor errors or total-budget exhaustion; inspect one next tree key to detect
exhaustion without delivering an extra node.

Explicitly roll back the read transaction on success and ordinary errors. A
cleanup failure is a backend error, never successful completion. Driver RAII
remains the panic/unwind fallback. Visitors must obey the existing non-mutating,
non-reentrant port contract. Each later scan/page call rereads authoritative
history, so a warm schema cache cannot hide changed database bytes.

No public DTO, store format, migration, new cache framework or dependency.
Reuse existing benchmark attribution; scan total includes visitor time, while
individual SQL/decode/validation regions do not. Query adoption and representative
latency evidence remain separate gates. Test callback/budget/error cleanup,
corruption before and between scans, snapshot behavior and backend/restart parity.

Validation: 49 regular Turso unit tests pass (one timing diagnostic ignored),
including transaction cleanup, warm-cache corruption and deferred snapshot
isolation. All 17 cross-backend/restart scenarios pass in 42.71 seconds.
All-target/all-feature Turso Clippy passes with warnings denied. These checks
establish correctness, not a cold-context speedup; Query still uses paged reads.

The first full contributor CI run passed architecture, extension import,
migration registries, workspace feature checks and strict workspace Clippy,
then failed daemon lifecycle test
`verification_reload_without_override_preserves_gap_and_fails_closed`:
the child exited successfully where the test expected failure. The test's
shared launcher imposes a five-second watch duration; deadline expiry is a
hypothesis, not an established diagnosis. A focused reproduction is running.
Full contributor CI remains unaccepted; do not infer scan failure or success
from this unrelated lifecycle result.

The focused reproduction passes in 0.84 seconds, producing the expected
verification-head error in native and polling modes. Remove the unrelated
five-second successful-shutdown path only from this expected-failure scenario;
the existing ten-second status wait and RAII process cleanup remain. All 14
daemon lifecycle tests subsequently pass in 1.80 seconds and strict daemon
Clippy passes. Production behavior is unchanged. Full contributor CI is rerunning.

The rerun completes successfully in 151.75 seconds with two build jobs,
including workspace feature checks, strict Clippy, all-feature tests, all 17
backend scenarios (45.19 seconds), the corrected daemon lifecycle scenario,
architecture/extension/migration gates and documentation. The optimized backend
scan is verified. Query adoption and representative latency remain open.
