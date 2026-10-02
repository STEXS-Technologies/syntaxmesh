# ADR-0314: Reuse bounded IN-query batching for known tree-page IDs

Status: rejected after representative measurement; implementation removed and full post-removal CI passed.

Post-removal full contributor CI passes in 211.08 seconds with two build jobs,
including all 17 backend/restart scenarios (40.48 seconds), strict workspace
Clippy, architecture checks, migration checks and documentation. The restored
demand-driven implementation is correctness-verified. No new post-removal
repository timing is claimed; the prior ADR-0313 measurement remains historical
evidence, not a fresh performance guarantee. Cold-query usability and broader
release/quality gates remain open.

Remove the private frontier, fixed-width batch reader, cached-ID set and
batch-only timing counters/tests. Restore the single demanded-page prepared
reader with ADR-0313 transfer/decode attribution. Retain oversized-node delivery
coverage and adapt the corruption fixture to the demand-driven contract: a
partial scan must reject corruption, release its transaction and recover fully
after repair; speculative rejection before any callback is no longer promised.
All 50 regular Turso tests pass (one timing diagnostic ignored), strict Turso
Clippy passes and full contributor CI is running. Evidence logs and this rejected
decision remain available; two prefetch-only source files are removed.

The completed matching-input whole-Sim bundle reports cold context at
51.671834 seconds versus ADR-0313's 13.324809 seconds. Index construction is
51.165979 seconds (49.719378 page-read attribution), with unchanged 87,486
nodes, 567,068 postings and 21,071,384 payload bytes. All five required
8,192-token targets remain retrieved; smaller budgets retain three of five.
The scan loads 583,879 pages through 51,649 batch queries versus the prior
90,556 loaded pages. Enclosing SQL time is 44.647337 seconds, including
32.781998 seconds transfer and 10.340961 seconds envelope decoding.
Repeated speculative loading and cache churn are a plausible explanation for
the transfer amplification, not yet a separately isolated causal proof.

The evaluator completes successfully in 522.30 seconds; publication takes
376.594 seconds. Reject this batching implementation: it does not earn its
complexity or memory cost in this representative sample. Remove the production
prefetch path and batch-only instrumentation/tests while preserving applicable
oversized-node and transaction-corruption coverage. Run full post-removal CI.
The borrowed SQL pattern itself is not disproven; this frontier/cache policy is
not accepted. One sample suffices to withhold adoption, not to claim a universal
performance ratio.

Full contributor CI subsequently passes in 142.35 seconds, including all 14
daemon lifecycle tests, 52 regular Turso tests and all 17 cross-backend/restart
scenarios (46.22 seconds). The speculative-corruption regression is included.
Start the matching-policy whole-Sim debug evaluation with one fresh temporary
database and indexed-host profiling. Performance acceptance remains pending;
CI does not establish speedup or remove the intermittent-test race hypothesis.

The live comparison bundle is
`target/context-retrieval-results/context-20261002T133908.273217Z-uncommitted`.
Its prepared inventory matches ADR-0313: 2,572 files and fingerprint
`d085ad193619f9cb5957280fc2d47f93562a3ea6156c0baa69ecaf1710c91d24`.
Metadata confirms repository scope, debug profile, indexed-host instrumentation
and the same scratch parent. Publication, index counts, target checks and
terminal timings are not yet available; this is partial evidence only.

The verification-policy reload lifecycle test passes in a fresh focused run for
native notifications and polling (0.82 seconds). Add failure-only diagnostics
with the observed HTTP status, watch mode and prior generation IDs to distinguish
a missed transition from a slow exit; production behavior and test acceptance
remain unchanged. Strict daemon Clippy and formatting pass. A fresh full
contributor CI run is active; its terminal result and the representative batch
benchmark remain required before accepting this optimization.

ADR-0313 measures SQL/row/BLOB transfer at 6.417444 seconds versus 1.732689
seconds envelope decoding. Reuse Shardline's transaction-local parameterized
IN-query pattern (`shardline-index/src/local_sqlite/helpers.rs`,
load_latest_verified_event_json_batch), adapted to content-addressed tree pages.
Keep SyntaxMesh's existing range walker, read transaction and validated-page
cache. This is speculative loading of already-known IDs, not another walker,
cross-generation cache, materialized graph or schema migration.

Maintain at most 64 deduplicated pending IDs discovered from children of pages
that passed existing content-address/envelope/priority validation. Do not discover
descendants from unvalidated bytes. Only enqueue child directions that can still
reach the node-key region; the existing walker remains authoritative for order,
node identity, visitor delivery and total node-budget checks.

When a demanded page is missing, include it in at most 16 known-ID candidates.
Use one prepared fixed-width IN query padded with NULL parameters. Speculative
rows must be BLOBs of at most 256 KiB, filtered in SQL before transfer, so raw
batch payload retention is at most 4 MiB plus bounded row/ID metadata. This is
separate from the existing 128-page/4 MiB charged validated cache, not an RSS
guarantee. Missing/oversized speculative pages are discarded from the pending
frontier; if later demanded, use the existing single-page read and missing-page
error. A demanded oversized page retains the existing transient exception.

Verify returned page IDs belong to the request and are unique. Decode and
validate all returned rows before reuse. Insert the demanded page last, so cache
eviction cannot discard it before the walker resumes. Invalid speculative bytes
fail the entire operation even if those pages would not yet have been delivered;
this stronger fail-closed behavior is deliberate. Missing speculative rows alone
are not errors. A visitor failure still stops immediately, without another SQL
query; no partial index is published. Rollback behavior remains ADR-0311's.

Before adoption, test ID/frontier limits and deduplication, oversized filtering
and demanded fallback, foreign/duplicate/corrupt/missing rows, cache eviction,
exact node budgets, visitor stopping, snapshot isolation and retained/restart
parity. Separate logical batch-query counts from loaded-page counts in private
profiling. Run strict Clippy, normal/instrumented checks, contributor CI and
matching-input whole-repository evaluation. Remove the optimization if its
complexity or memory cost does not earn a representative benefit. No speedup,
O(1) query behavior or default promotion is established by this decision.

The initial private frontier and fixed-width batch reader are connected to the
transaction-scoped scanner. The all-feature check passes. Existing Turso unit
tests and strict Clippy are running; additional frontier/SQL boundary tests,
batch transfer/decode attribution, full CI and representative benefit remain
pending. Do not infer adoption readiness from compilation alone.

Initial 49 regular Turso tests and strict Clippy pass. Subsequent focused tests
pass for frontier deduplication/cap/key directions, batch width, demanded-last
ordering, SQL oversized filtering and absent speculative rows. Remove page
clones during insertion; carry only copied key/child metadata until validation
succeeds. Add cached-ID bytes to logical cache charges (not allocator overhead).
Full Turso/backend tests and final strict Clippy are running. Foreign/duplicate
rows, speculative corruption, fallback/eviction regressions, complete batch
transfer/decode attribution, contributor CI and benefit remain open.

All 17 backend/restart scenarios subsequently pass in 41.34 seconds; strict
Clippy passes before the next test additions. Add SQL duplicate-row, foreign-ID
and corrupt-Bincode rejection checks and enabled/disabled batch-query versus
loaded-page counter assertions. Their focused tests/final Clippy are running.
Batch-specific transfer/decode timing and demanded-oversize/speculative-integrity
integration coverage remain required before the representative benchmark.

Batch SQL/row/BLOB transfer and Bincode decode subregions are now wired through
the same feature/environment-gated profile, preserving enclosing batch time and
separate query/page counts. The real-store cleanup/budget/callback/history test
now publishes and delivers a 300 KB node, exercising demanded oversized fallback.
All 51 regular Turso tests pass (one diagnostic ignored), strict Clippy and
no-default-feature compilation pass. Full contributor CI and normal-build
prefetch tests are running. Explicit speculative-corruption integration and
representative measured benefit remain open; no benchmark is claimed yet.

Normal-build prefetch tests pass. Extend the SQL/cache component regression:
a decodable non-demanded speculative row under an unrelated content address
is rejected as CorruptPage and is not retained. The focused regression and
strict Clippy pass. This establishes decode-to-cache validation, not a full
scanner rollback assertion for speculative corruption; that integration gate
remains open while the existing contributor CI run proceeds.

The full-scan speculative-corruption regression now passes: corrupt a known
right-branch page while descent seeks the first left-side node. The scan fails
before callbacks, releases its transaction, and visits all 128 nodes after the
page is repaired. Final strict Clippy is running. Contributor CI stopped on the
daemon verification-reload scenario (`daemon did not stop`), not on Turso tests.
A focused daemon reproduction is running; policy/source observation ordering
is a hypothesis, not yet a confirmed diagnosis. Full CI remains unaccepted.
