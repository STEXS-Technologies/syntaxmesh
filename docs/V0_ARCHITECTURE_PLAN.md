# SyntaxMesh v0 architecture and implementation plan

Status: proposed execution plan, 2026-09-27. The two source-of-truth documents in this directory remain authoritative for product intent. This plan makes the first implementation slice and its exit gates explicit.

## 1. Scope and release distinction

### Current execution priority: the complete v0–v5 vision

ADR-0330 records the next higher-roadmap slice: optional restricted deterministic
inference above canonical facts, reusing Slotstrike's pure specification pattern
and existing SyntaxMesh identity/provenance conventions. Existing consequence
mutation validates explicit evidence only; it is not a rule evaluator. The new
inference contract and runnable evaluator remain unimplemented. Acceptance must
include real path/join evaluation, bounded failure and premise invalidation,
not DTO-only scaffolding.






The user's priority is progress across the full source-of-truth vision, not an
indefinite focus on one ingestion edge case. The document's v2–v5 expansions
remain product requirements, not claims that those releases have shipped.
Use the original sections 72/73, 77, 127, 130 and 230/316 when selecting work.
The table below is a capability map, not a completed release audit.

| Layer | Existing foundation | Required next evidence/capability |
| --- | --- | --- |
| v0 | Rust-native supported-language/docs extraction, incremental graph, durable Engine, stores, search/traversal/context and hosts | Close release gates with representative end-to-end ingestion/retrieval and operational evidence; finish visible partial-processing integration |
| v1 | Migration registries, public DTOs/SDKs, independent extension fixtures and restart/conformance tests | Audit stability promises, production/cross-platform operations, workspace/service and interoperability requirements individually |
| v2 | Provenance-backed documentation semantics and semantic job/cache workflows | Implement and verify the higher-level ontology, derivation, contract/drift and explanation requirements; do not equate AI triples with complete reasoning |
| v3 | Distinct static/runtime evidence and public extension ingestion | Verify engineering-knowledge reconciliation across the fact, knowledge, reasoning and delivery planes using real supported workflows |
| v4 | Embeddable Engine, context APIs, MCP and local selectable-model harness integration | Demonstrate native harness integration and reproducible workflow benefit without privileged Codex paths or runtime coupling |
| v5 | Indexed retained fact history, generation-scoped queries and explicit lineage/consequence primitives | Implement and verify temporal reasoning, premise/consequence propagation and the remaining staged temporal-intelligence requirements |

Prioritize dependency-unblocking vertical slices that cross these layers. Reuse
applicable patterns from sibling projects, with source evidence and tests; do not
copy dependencies across pure-core boundaries. Do not report a layer complete
from a narrower fixture or substitute synthetic benchmarks for workflow benefit.
Keep implementation, verification and missing capability distinct in updates.



Full contributor CI for this accumulated embedded coverage implementation passes
in 274.56 seconds, including all 17 backend conformance scenarios (45.55 seconds)
and shared processing lifecycle fixtures. This verifies the tested slice only;
the roadmap map above is not a declaration that any entire version is complete.


ADR-0325 full contributor CI passes in 240.30 seconds, including all 17
backend/restart scenarios and shared File/Turso failed-update/repair history
fixtures. Indexer context preserves the syntax-error classification; partial
ingestion remains open and strict publication stays atomic.


ADR-0324 composes explicit role packing in the indexed MCP opt-in through Query,
without changing default or explicitly seeded requests. Host clone/cache/reset,
response-level ordering and all-role explicit-seed parity pass full contributor
CI (149.74 seconds). An additional publication/reopen fixture verifies latest,
superseded and latest role ordering with token and generation bounds; its focused
test passes. Corpus quality and full CI after that fixture remain open.

Matching-source whole-Shardline release evaluation with exact function targets
retains all six large-budget targets under neutral and explicit test-last.
Small-budget matches are four versus five (benchmark documentation gain).
An earlier substring-target collision mistakenly selected test functions; the
evaluator now requires exact function identity and has regression coverage.
Default stays neutral; test-oriented and broader corpus quality gates remain open.

Expanded seven-case Shardline evidence includes an exact qualified test target:
neutral retrieves 7/7 large-budget and 5/7 small-budget targets; test-first 7/7
and 2/7; test-last 6/7 and 5/7, failing the required test target. Keep neutral
default and explicit role preference as opt-in composition only. These are
single-run quality observations, not repeatable performance evidence; broader
repository quality remains open.

Final full contributor CI for ADR-0324 and the seven-case qualified-target matrix
passes in 135.08 seconds, including all 17 backend/restart scenarios (41.89 seconds).
This verifies implementation consistency, not default promotion of role ordering.

ADR-0323 adds explicit neutral/test-intent-first/test-intent-last packing order
over the existing complete plan. All 61 Query tests and strict Clippy pass
with exact neutral parity and membership retention. Positive roles are decoded
through the pure SDK and charged to index budgets, with exact boundary and
malformed-payload coverage. Full CI passes in 257.44 seconds, including all 17
backend/restart scenarios; corpus quality remains pending;
host defaults stay neutral.

ADR-0322 extends the existing AST/SDK role path to direct qualified Tokio and
async-std attributes, preserving callable identity and unknown-role semantics.
All 34 Rust tests, both File/Turso history scenarios and strict focused Clippy
pass. Full CI passes in 179.32 seconds, including all 17 backend/restart
scenarios; no framework executes and ranking is unchanged.

ADR-0321 switches Rust test intent to the shared SDK codec and producer 0.17.0.
All 33 Rust tests and both File/Turso shared history scenarios pass, including
legacy-role-to-canonical-role upgrade on unchanged source with retained verified
history. Strict Rust/Engine/Turso Clippy passes. Full CI passes in 182.91 seconds,
including all 17 backend/restart scenarios (42.20 seconds), and
ranking does not yet consume source-role evidence.

ADR-0320 reuses the SDK's existing versioned metadata codec pattern for typed
source roles. Canonical and legacy Rust intent decode without coupling generic
consumers to the Rust extractor; unknown is not production-code evidence.
All nine SDK tests and strict SDK Clippy pass. Full contributor CI passes in
217.27 seconds, including all 17 backend/restart scenarios (40.15 seconds).
Producer adoption and ranking consumption remain open.

ADR-0319 adds direct standard Rust `#[test]` intent as versioned producer metadata
on unchanged callable function identities, using the existing AST/declaration
maps. All 33 Rust extractor tests and strict Rust Clippy pass. Framework and
conditional test attributes remain unclassified; absence is not proof of
production code. The File producer-upgrade/restart fixture verifies role addition,
removal and retained verified history. The same generic fixture also passes on
Turso, with strict Engine/Turso Clippy. Initial metadata/File full CI passes in
181.87 seconds; final shared-fixture layout CI passes in 133.57 seconds,
and context ranking does not yet consume this metadata.

ADR-0318 reuses complete exact/stemmed scoring across family partitions rather
than repeating it four times. The existing ordering and logical posting-budget
charges are preserved. All 59 Query unit tests and strict Query Clippy pass,
including independent-oracle and crowded-family cutoff coverage. Full contributor
CI passes in 184.90 seconds, including all 17 backend/restart scenarios
(42.58 seconds). Matching-input release evaluation retains all six larger-budget
and four smaller-budget targets with unchanged index/token counts; warm
larger-budget queries measure 36–43 ms in one sample. Repeatable speedup is not
established and small-budget relevance is unchanged.

The expanded six-case committed-Shardline release evaluation passes with unchanged
inputs/index counts. Pending Rust migrations are retrieved at both budgets;
all six larger-budget targets and four of six smaller-budget targets are retained.
Existing webhook/heading misses remain. Full contributor CI passes in 139.57
seconds for the evaluator module move and fixture/count expansion; production
lookup is unchanged and the remaining retrieval-quality gate stays open.

The committed-Shardline retrieval matrix now adds a second Rust workflow case:
natural-language lookup of `apply_pending_local_migrations` in the existing
SQLite helper implementation. Existing webhook, Python, Bash and documentation
cases are retained; the evidence harness requires six complete case pairs for
Shardline. This expands quality coverage without changing production lookup.
The query order changes, so older cold-case timings are not isolated comparisons.
Focused validation and the expanded clean-corpus release run remain required.

The large opt-in MCP retrieval evaluator moves from `server/tests.rs` into its
focused `tests/context_evaluation.rs` child, reusing Shardline's sibling test
layout. Existing fixture helpers remain shared; ranking, packing and target
matrices are unchanged. All 27 regular MCP tests, strict MCP Clippy and formatting
pass; substring-based benchmark discovery still finds the moved evaluator.
This is maintainability work, not additional retrieval-quality acceptance.

ADR-0317's separate clean committed-HEAD Shardline repetition passes on identical
808-file inputs and index counts: cold context 2.67–2.69 seconds, publication
median 44.145 seconds, and warm required-budget queries 41–65 milliseconds.
All 15 larger-budget target checks pass, including documentation rationale;
smaller budgets retain 9/15. This does not validate uncommitted sibling edits
or close broader quality/default-promotion gates.

ADR-0317 whole-Shardline release repetition rejects changed inputs after sample
two; no accepted aggregate is produced. The first 908-file raw sample retains
all five required targets, including rationale content, with cold context
3.119448 seconds and warm required-budget queries 44–68 milliseconds. This is
single-sample evidence only; stable-corpus repetition remains open and fingerprint
validation must not be suppressed to accept a moving sibling worktree.

ADR-0316 full contributor CI passes in 146.20 seconds, including all 17 backend
scenarios (46.92 seconds), daemon lifecycle tests and strict checks. Both missed
targets are original lexical seeds; generic lexical ranking also places Python
poorly. The diagnostic work is verified, but production small-budget ordering
and broader retrieval quality remain open.

ADR-0316 reuses the existing target and family-plan probes at both token budgets.
Clean captures show Python/Quickstart fit at 2,048 tokens when directly selected,
but the ordinary family plan admits earlier test/doc evidence and omits them
without source warnings. Regular MCP tests and strict Clippy pass; full CI is
running. Production ranking remains unchanged; broader query-quality evaluation
and a general ordering policy remain open.

ADR-0315 three fresh-database release repetitions complete on matching inputs:
cold context 1.01–1.11 seconds (median 1.036111 seconds), warm 8,192-token
queries 35–59 milliseconds and publication median 16.312 seconds. All required
targets and index counts remain stable; small budgets retain three of five.
This is limited same-host consistency, not broad relevance, tail latency or
default promotion. Further work should prioritize representative query quality.

ADR-0315's first whole-Sim release sample completes on matching 2,572-file
inputs and identical index counts: publication 17.663 seconds, cold context
1.033233 seconds and warm 8,192-token queries 36–53 milliseconds. All five
required targets remain retrieved; smaller budgets retain three of five.
This is optimized-build baseline evidence, not an algorithmic speedup or
repeatable latency acceptance. Broader retrieval quality/default promotion
remain open; no new cache is justified by this result alone.

ADR-0315 starts an optimized-release whole-Sim baseline using the existing
provenance-captured evaluator and restored demand-driven reader. Shardline's
bounded FIFO pattern is inspected but not copied without evidence of benefit.
Release latency, index/target parity and input matching remain pending; debug
timings are not deployment-latency guarantees.

ADR-0314 post-removal full contributor CI passes in 211.08 seconds, including
all 17 backend/restart scenarios (40.48 seconds), strict Clippy and documentation.
The demand-driven reader is restored and verified; rejected speculative batching
is absent. Cold-query usability remains open, and no fresh post-removal latency
claim follows from correctness checks.

ADR-0314's rejected speculative prefetch implementation is removed. The restored
demand-driven reader passes all 50 regular Turso tests, retaining oversized-node
and corruption rollback/repair coverage. Strict Turso Clippy passes; full
post-removal contributor CI is running. The benchmark evidence is retained.

ADR-0314 is rejected after matching-input whole-Sim measurement: cold context
regresses to 51.671834 seconds from 13.324809 seconds, with unchanged index
counts and all five required targets retained. Loaded pages rise to 583,879
from 90,556. Remove the speculative batching path and verify the restored reader;
post-removal CI remains pending. No batching speedup is accepted.

ADR-0314 reuses Shardline's parameterized IN-query pattern for bounded loading
of already-known tree-page IDs. Full contributor CI passes in 142.35 seconds,
including all 17 backend/restart scenarios, oversized-demand fallback and
speculative-corruption rollback coverage. The matching-policy whole-Sim debug
evaluation is running; measured benefit and optimization acceptance remain open.

ADR-0313 whole-Sim attribution completes with unchanged index counts and all
five required targets retained. Of 8.232209 seconds enclosing page-read time,
6.417444 seconds are SQL/row/BLOB transfer and 1.732689 seconds envelope decode.
Cold context remains 13.324809 seconds in this debug sample, not a new claimed
speedup. Bounded known-ID request batching is the next candidate; it requires
explicit frontier and integrity/budget rules, not blind subtree prefetching.

ADR-0313 adds private nested SQL/transfer and Bincode envelope-decode timing
inside the existing node-read profiler; normal reads remain unchanged. Full
contributor CI passes in 199.94 seconds, including all 17 backend scenarios.
Whole-Sim attribution is live on unchanged inputs. Choosing the next scan
optimization awaits this breakdown; no additional speedup is claimed.

ADR-0312 whole-Sim evaluation completes with all five required targets retained
on the unchanged 2,572-file fingerprint and identical index counts. Cold context
is 13.819686 seconds versus ADR-0307's 37.411926 seconds in one debug sample;
the authoritative history BLOB is read once instead of 88 times. Index build
is 13.322831 seconds, with SQL page loading now dominant at 8.527266 seconds.
This establishes a useful observed reduction, not repeatable latency acceptance,
general relevance or default promotion. Cold usability remains open; bounded
page-reader reuse is the next measured optimization candidate.

ADR-0312 full contributor CI passes in 198.28 seconds, including all 17 backend
scenarios, strict Clippy and documentation. The visitor-based index builder is
correctness-verified. Whole-Sim evaluation is live on the unchanged 2,572-file
input fingerprint; representative latency and retrieval-target results remain
pending. No default promotion or performance acceptance follows from CI alone.

ADR-0312 routes identifier-index builds through the validated visitor, preserving
existing postings and planner logic. Normal/instrumented Query tests, strict
Query Clippy, 27 MCP tests and three cross-backend context scenarios pass. A
restricted-store test verifies visitor use and original callback error retention.
Full CI and matching-policy whole-Sim retrieval are running. Logical scan counts
are distinct from physical page reads; no speedup is claimed before measurement.

ADR-0311 full contributor CI passes in 151.75 seconds, including all 17 backend
scenarios and the daemon verification-reload test with its competing timed
success path removed. Production daemon behavior is unchanged. Transaction-scoped
Turso traversal is verified; ADR-0312 Query adoption and matching-input latency
evaluation are next. No retrieval speedup is established yet.

ADR-0311 implements the bounded Turso visitor in one deferred read transaction,
reusing the existing walker, prepared reader and validated-page cache. All 49
regular Turso tests, all 17 backend scenarios and strict Clippy pass; cleanup,
corruption rejection and snapshot isolation have focused coverage. Full
contributor CI remains pending. Query adoption is still required before this
can reduce repeated history-record reads during context index construction.

ADR-0310 full contributor CI passes in 214.66 seconds with two jobs, including
all 17 backend scenarios and documentation. The object-safe reference visitor
is verified; optimized Turso traversal and Query integration are the next
implementation steps. No cold-retrieval speedup is established by this port.

ADR-0310 adds an object-safe bounded historical-node visitor, borrowing
Shardline's visitor-store pattern. Its reference path reuses existing pages;
cross-backend/restart result/budget/visitor-error checks, 34 Store tests, strict
Store/Turso Clippy and architecture checks pass. Transaction-scoped Turso
traversal and Query adoption remain pending, so cold retrieval is unchanged.

ADR-0309's isolated WAL diagnostic compares native page-cache suggestions:
five 33.6 MB reads take 1.230441 seconds at the pinned driver's default
`-2000` versus 1.075549 seconds at `-65536` in one debug sample. This does
not justify changing production memory policy or establish repository latency.
The diagnostic and 47 regular Turso tests pass; strict Clippy and formatting
pass. Full-record processing remains open; bounded single-operation historical
scanning is the next candidate to assess against existing store contracts.

ADR-0308 post-removal full contributor CI passes in 229.41 seconds with two
jobs, including all 17 backend scenarios and documentation. The restored
digest-only schema cache and retained corruption/type/deletion regression are
verified. Repeated full history-payload processing remains the measured open
bottleneck; the rejected exact-byte cache is not part of the implementation.

ADR-0308 is rejected after its completed whole-Sim comparison: cold context
is 40.948480 seconds versus the previous 37.411926 seconds, with identical
counts and all five required targets retained. SQL equality still processes
the large payload and adds retention memory. The optimization is removed;
ADR-0307 profiling and an adapted corruption regression remain. Post-removal
validation is pending; reducing full history processing remains open.

ADR-0308 full contributor CI passes in 280.79 seconds with two build jobs,
including all 17 backend scenarios and documentation. Its whole-Sim comparison
remains live; measured performance benefit and cold-query acceptance are open.

ADR-0308 implements a one-entry, 64 MiB-capped exact-history-byte cache using
SQL equality against the authoritative BLOB before schema reuse. Changed/missing
rows retain fail-closed handling; oversized payloads keep the previous path.
All 48 Turso unit tests, all 17 backend scenarios and strict Turso Clippy pass.
Full CI and matching-input whole-Sim timing are running; benefit is unproven.

ADR-0307 whole-Sim capture completes in 519.12 seconds with all five required
targets retained. Its 88 schema reads transfer 2,955,107,584 cumulative bytes
from one 33,580,768-byte history record, spending 22.143901 seconds in
SQL/transfer versus 1.911296 seconds hashing/decoding. Repeated full-record
transfer is the next optimization target. Cold context remains 37.411926
seconds; compact reads must preserve authoritative mutation/corruption checks.

ADR-0307 full contributor CI passes in 194.28 seconds with two build jobs,
including all 17 backend scenarios and documentation. Whole-Sim transfer versus
hash/decode attribution remains live; no cold-query optimization is claimed.

ADR-0307 separates history payload SQL/transfer time from hash/decode time
inside existing opt-in instrumentation. All 46 Turso unit tests, including
warm-cache corruption detection, strict Clippy, formatting and the normal
no-default-feature check pass. Whole-Sim capture is running; schema-read
optimization and its representative benefit remain pending.

ADR-0306 whole-Sim finer profiling completes successfully in 539.65 seconds,
retaining all five required 8,192-token targets. The 40.658077-second index
build spends 39.352074 seconds in node-page reads. Across its 88 scans,
authoritative schema reads account for 26.403471 seconds and SQL page loading
for 10.735801 seconds. Repeated history-payload loading is the next measured
target, with corruption detection preserved. Cold context remains 41.239454
seconds; this attribution does not establish performance acceptance or promotion.

ADR-0306 full contributor CI passes in 228.09 seconds with two build jobs,
including all 17 backend scenarios and documentation. Whole-Sim finer backend
profiling remains live; repository-scale schema/SQL/validation attribution is
still pending and no new speedup is claimed.

ADR-0306 adds private feature-gated backend attribution without changing store
ports or normal-build logging. Selected-file Sim retains all required targets
and emits schema/root, SQL-page, validation and fact-decoding timings. 46 Turso
tests, 16 xtask tests, strict Clippy and formatting pass; full CI and representative
finer profiling remain pending.

ADR-0305 whole-Sim comparison passes all five required targets with identical
index counts. Build time is 38.428678 seconds, including 37.201757 seconds
page reads, versus 42.733786/41.671530 seconds previously. This is one modest
matching-input reduction, not statistically established speedup or cold-query
acceptance. Finer backend attribution is next; page reads still dominate.

ADR-0305 full contributor CI passes in 252.84 seconds with two build jobs,
including all 17 backend scenarios and documentation. Whole-Sim before/after
profiling remains live with matching corpus/instrumentation/scratch policy;
representative speedup and cold-query acceptance remain unproven.

ADR-0305 reuses bounded entry/byte retention principles for the existing
scan-local persistent node-page cache (128 pages/4 MiB charged payload).
44 Turso tests, all 17 backend scenarios and strict Clippy pass; selected-file
Sim still passes all five required targets. Full CI is running and representative
performance remains pending; no new cross-generation cache or public contract.

ADR-0304 whole-Sim profile completes successfully in 500.65 seconds and still
passes all five required 8,192-token targets. The 87,486-node index takes
42.733786 seconds: 41.671530 seconds in store node-page reads and 1.061990
seconds in term/posting processing. The next measured target is backend page
reading/validation, not tokenization. Cold usability and default promotion
remain open; instrumentation/storage differences prevent isolated speedup claims.

ADRs 0303–0304 full contributor CI passes in 209.29 seconds with two build jobs,
including all 17 backend scenarios, mixed instrumentation checks and documentation.
Whole-Sim instrumented profiling remains live on its unchanged 2,572-file source
fingerprint; repository-scale cold-build attribution is pending.

ADR-0304 captures index-build metrics from the actual host cache without a
second index or oracle work. Selected-file Sim still passes all five required
targets and captures one 545-node build: 98.815 ms total, 90.381 ms page reads,
8.220 ms processing. 27 MCP/16 xtask tests and strict Clippy pass; full CI and
representative profiling remain pending. Normal host builds remain uninstrumented.

ADR-0303 adds feature-gated index-build metrics using the existing Query
benchmark convention: node-page read time and term/posting processing time are
separate from total elapsed time. All 58 instrumented/57 normal Query tests,
strict Query Clippy and formatting pass. Representative host capture/profiling
and full CI remain pending; no cold-query optimization is claimed from metrics.

ADR-0301 disk-backed whole-Shardline evaluation completes successfully in
1,357.01 seconds on 844 files: all five required 8,192-token targets pass and
8/10 targets pass across both budgets. Cold context takes 182.823160 seconds;
retained 8,192-token requests take 0.902398–1.062620 seconds in one debug sample.
The fixed-target host gate now passes on both Sim and current Shardline, but
cold-query usability, general relevance and default promotion remain open.
Changed corpus/storage prevent a same-input statement-reuse speedup claim.

ADR-0301 disk-backed whole-Shardline publication succeeds in 970.761 seconds
on the 844-file corpus. Host setup/retrieval remains live; the quota-failing
publication stage is cleared without changing the workload or deleting user data.

ADR-0302 full contributor CI passes in 128.16 seconds with two build jobs,
including all 17 backend scenarios and documentation. The 844-file Shardline
disk-backed evaluation remains live; retrieval quality/performance gates remain
open independently of contributor CI.

ADR-0302 rejects indexed-host benchmark policy without an explicit external
fixture root, preventing engine-only results from being mislabeled. All 16
xtask tests, strict Clippy, formatting and real-command early rejection pass.
The explicit-root disk-backed Shardline evaluation remains live and unchanged.

ADR-0301 whole-Shardline indexed-host evaluation is running with disk-backed
temporary storage outside the source tree. Its prepared corpus is now 844 files;
the changed fingerprint prevents unchanged-input speedup comparisons. Existing
benchmark metadata records the scratch root; no new scratch-storage framework
or production contract was added.

ADR-0301 explicit selected-file Sim indexed-host smoke passes all five required
8,192-token targets on the unchanged five-file fingerprint. This verifies the
host smoke after statement reuse, not repository-scale latency improvement.

ADR-0300 current-Shardline actual-host evaluation fails during publication with
`pwritev: quota exceeded` after 728.07 seconds on the changed 842-file corpus.
It produces no retrieval result; the cause and a successful rerun remain open.

ADR-0301 full contributor CI passes in 205.66 seconds with two build jobs,
including all-feature workspace tests, all 17 backend scenarios and documentation.
Prepared-statement reuse is correctness-verified; representative cold-query
performance remains unaccepted. The ADR-0300 Shardline baseline remains live.

ADR-0301 reuses the existing Turso incidence-reader prepared-statement pattern
for historical node scans without changing cache retention or public contracts.
All 41 Turso unit tests, all 17 cross-backend conformance scenarios, strict
Clippy, formatting and architecture checks pass. Performance evaluation and
full contributor CI for this change remain pending.
The already-compiled ADR-0300 Shardline baseline scans a changed 842-file corpus;
its artifact retains the fingerprint, so earlier 828-file timings are not a
same-input comparison.

ADR-0300 whole-Sim actual-host
`context-20261002T100218.001743Z-uncommitted` passes all five required
8,192-token targets with one hop/64 candidates; ten budgeted requests admit
8/10 targets. Cold context takes 78.732362 seconds including index construction;
retained 8,192-token requests take 0.718761–0.974735 seconds in this one debug
sample. Publication is 387.895 seconds. Current-Shardline host evaluation is
running. Default routing and cold-query usability remain unaccepted.

ADR-0300 benchmark-integration CI passes in 130.42 seconds after correcting
the metadata test for the ninth policy flag; all 15 xtask tests pass. Whole-Sim
actual-host evaluation remains live. Selected-file success and CI do not close
whole-repository host quality or default promotion.

ADR-0300 selected-file actual-host Sim run
`context-20261002T100153.512522Z-uncommitted` passes all five required
8,192-token targets with one hop and no oracle work. Cold/retained cache state
is explicit; small-fixture timings do not prove repository-scale latency.
Whole-Sim host evaluation and benchmark-integration CI are running;
current-Shardline host validation and default promotion remain open.

ADRs 0297–0299 implement a bounded clone-shared MCP planner cache and explicit
opt-in indexed context, including empty selected-set packing without fallback.
27 MCP and 57 Query tests and strict Clippy pass. Live publication refresh,
historical source warnings, explicit-seed preservation and small caller bounds
are covered. Contributor CI is running. ADR-0300 records separate actual-host
quality/latency evaluation with unchanged one-hop/64-candidate cases; that
integration/evidence is pending and default routing is unchanged.

ADR-0296 current whole-Shardline
`context-20261002T091538.461442Z-uncommitted` passes 5/5 indexed seed/packing
oracle comparisons and 5/5 diagnostic admissions. Publication is 972.347 seconds,
index construction 273.484769 seconds; isolated composition is 13.434–91.534 ms.
Default retrieval remains 6/10, missing Python metadata, and the benchmark still
fails that gate. Together with whole-Sim and contributor CI this verifies the
opt-in indexed diagnostic; production host caching/routing and end-to-end
performance remain open. The changed Shardline corpus prevents same-input timing
claims against the earlier run.

ADR-0296 current whole-Shardline run
`context-20261002T091538.461442Z-uncommitted` prepares 828 files, differing
from the earlier 810-file corpus. Its fingerprint is retained in the ADR and
artifact; outcomes must not be described as an unchanged-input performance
comparison. Publication remains live.

ADR-0296 whole-Sim `context-20261002T090339.373794Z-uncommitted` passes
5/5 indexed seed/packing oracle comparisons and 5/5 diagnostic admissions on
the unchanged 2,572-file corpus. Publication is 410.490 seconds, index build
78.094302 seconds; isolated indexed composition is 2.504–29.369 ms. The command
still fails the unchanged default gate (3/10). Whole-Shardline regression is
running; default production routing remains unchanged.

ADR-0296 full contributor CI passes in 210.39 seconds with two build jobs,
including all 17 backend conformance scenarios and documentation. Whole-Sim
indexed retrieval remains live; representative whole-repository quality and
default routing are not yet accepted.

ADR-0296 implements thin Query access and oracle-checked indexed family
diagnostics. 57 Query tests, 23 MCP tests and strict Clippy pass. Selected-file
Sim `context-20261002T090300.307836Z-uncommitted` admits 5/5 targets with exact
seed/packing oracle equivalence; individual indexed compositions take about
2–3 ms on this small fixture. Whole-Sim indexed evaluation is running;
whole-Shardline, integration CI and default routing remain open.

ADR-0295 full contributor CI is reverified green in 693.64 seconds with two
build jobs, including backend conformance and documentation. ADR-0296 Query
adapter implementation/testing now proceeds separately; representative indexed
retrieval quality and default routing remain unaccepted.

ADR-0296 records the next additive boundary: thin generation-pinned Query access
to the existing planner index and ID-only composer, followed by indexed family
diagnostic seed/packing equivalence checks against the canonical corpus oracle.
Implementation and representative indexed-path measurements remain pending;
default host routing is unchanged. ADR-0295 contributor CI is being reverified
with a retained log because the previous process handle disappeared without a
recoverable terminal result; its targeted checks are not a full-CI claim.

ADR-0295 composer capacity (256/257 selected, 32/33 original seeds) and temporal
four-backend/reopen coverage pass with all 56 Query tests and strict Query/Turso
Clippy. The existing fixture now compares indexed seed and packing plans across
family/label/path edits. Full CI is running; representative indexed-path quality
and default host routing remain unaccepted.

ADR-0295 implements ID-only indexed family seed and complete discovery-balanced
packing composition, reusing the verified lexical index and diagnostic refill
policy. All 55 Query tests, strict Clippy and architecture checks pass, including
independent corpus-order equivalence and manifest-only/no-hydration work checks.
Temporal composer/backend and capacity coverage, full CI and representative
indexed-path quality remain open. Default host routing is unchanged.

ADR-0294 full contributor CI completes successfully in 290.57 seconds with two
build jobs after generated-artifact cleanup resolved disk/linking failures.
All 17 backend conformance scenarios and workspace gates pass. This verifies
the opt-in lexical index capability; indexed composer integration and
representative host-quality measurements remain open. Default routing is unchanged.

ADR-0294 extends the existing four-backend path-index temporal fixture with
exact/stemmed family and eligible-set rankings, label/path edits, a code-to-doc
family change, retained cached indexes and durable reopen. Targeted conformance,
all 52 Query tests and strict Query/Turso Clippy pass. Full contributor CI is
running; indexed composer integration and host-quality verification remain open.

ADR-0294 implements opt-in exact/stemmed family ranking in the existing generation
identifier index, built in one pinned page walk with explicit postings/metadata
budgets. Independent global-oracle and restricted-port query tests pass alongside
all 52 Query tests and strict Clippy. Existing exact/path APIs retain their
behavior; production routing is unchanged. Temporal backend coverage, planner
composition and indexed-path representative quality remain open.

ADR-0293 whole-Shardline `context-20261002T035749.640093Z-uncommitted`
finishes with diagnostic admission 5/5, retaining Rust webhook and retrieving
Python metadata, Bash and both documentation targets. Publication takes
935.195 seconds; identifier indexing takes 248.521774 seconds. Default retrieval
remains 6/10 and fails on Python metadata. With whole-Sim diagnostic 5/5 and
full contributor CI green, the next work is an opt-in indexed equivalent under
ADR-0294, not production promotion of the complete-corpus diagnostic oracle.

Full contributor CI for ADR-0293 passes in 130.72 seconds, including all 17
backend conformance scenarios and workspace gates. Whole-Shardline regression
`context-20261002T035749.640093Z-uncommitted` remains running. The whole-Sim
diagnostic 5/5 does not yet close regression quality or production integration.

ADR-0293 whole-Sim `context-20261002T034707.306262Z-uncommitted` completes
with diagnostic source admission 5/5 at 8,192 tokens, including the previously
omitted TypeScript caller. The unchanged corpus publishes in 362.992 seconds;
identifier indexing takes 76.521950 seconds. Default retrieval remains 3/10
and fails its required-target gate. Shardline regression and production
planner/index integration remain open; diagnostic success is not promotion.

ADR-0293 adds discovery-balanced diagnostic packing by reusing the existing
complete selected-set plan, round-robin refill helper and ranked-plan compiler.
Lexical seeds and graph discoveries alternate before stable primary-evidence
priority; membership, candidate/token bounds and production routing are unchanged.
23 MCP tests and strict Clippy pass. Selected-file run
`context-20261002T034640.362077Z-uncommitted` admits 5/5 source targets.
Whole-Sim `context-20261002T034707.306262Z-uncommitted` is running; this does not
close representative retrieval quality or promote a host policy.

ADR-0292 whole-Sim `context-20261002T033322.188476Z-uncommitted` completes:
complete-selected-set diagnostic admission remains 4/5 at 8,192 tokens;
TypeScript still misses packing. Default retrieval stays 3/10 and fails its
required-target gate. Publication takes 359.372 seconds and identifier indexing
75.494537 seconds for the unchanged 2,572-file corpus. Set retention is verified,
but evidence allocation remains open; no production routing is promoted.

ADR-0292 integrates the complete bounded selected-set diagnostic with ADR-0291's
packing-plan API. All selected IDs survive ordering, including zero-score nodes;
no second traversal or candidate/token-bound increase occurs. 22 MCP tests and
strict Clippy pass, including exact membership and 256/257 capacity checks.
Selected-file run `context-20261002T033234.720067Z-uncommitted` admits 5/5 target
sources at 8,192 tokens. Whole-Sim verification is running; this is not production
routing or representative quality acceptance.

Full contributor CI for ADR-0291 bounded ranked packing plans and the 40-distinct
temporal fixture passes in 176.93 seconds, including all 17 backend scenarios
and contributor gates. This closes compiler/backend verification. Integration of
a complete selected-set diagnostic planner and representative retrieval quality
remain separate open work; the rejected 64-to-32 refinement is not promoted.

ADR-0291's existing temporal backend fixture now verifies a 40-distinct-node plan
with one duplicate, expected priorities, complete source attribution and exact
serialized budgets. Current/historical packs match across all four stores and
retain equality after source edits and durable reopen. Targeted conformance and
strict Query/Turso Clippy pass; full contributor CI is running. This is typed
compiler coverage, not repository-quality acceptance or production routing.

ADR-0291 implements additive current/historical bounded ranked packing-plan APIs
using the shared compiler. They require zero hops and accept up to max_candidates
raw plan entries (maximum 256); old seed APIs retain the 32-entry limit. All 49
Query tests and strict Query Clippy pass, including duplicate priority, exact
current/history parity and invalid-plan rejection. More-than-32-distinct retention,
four-backend/restart conformance, full CI and retrieval-quality verification remain
open. No default host policy is promoted.

ADR-0290 whole-Sim `context-20261002T031036.487096Z-uncommitted` completes and
rejects 64-to-32 round-robin refinement: diagnostic source admission stays 4/5,
but TypeScript disappears from the final plan despite initial depth-one selection.
Default quality remains 3/10. Publication takes 364.699 seconds and identifier
indexing 76.579712 seconds. Do not repeat this unchanged on Shardline or promote
selected-file 5/5. The next contract needs bounded selected-set retention with
explicit packing priorities, not another seed-budget reduction hiding graph facts.

ADR-0290 adds a diagnostic graph-refined plan: select at most 64 nodes through
the existing ranked graph selector, then reuse globally scored family channels
restricted to those IDs to compose at most 32 ranked seeds. Packing uses zero
additional hops; the ranked compiler contract is unchanged. This may replace
original seeds and is not identical-membership ordering. Corrected selected-file
run `context-20261002T030955.284788Z-uncommitted` admits all five source targets;
22 MCP tests and strict Clippy pass. Whole-Sim verification is running; no default
host policy or public contract is promoted.

Whole-Shardline primary-first diagnostic
`context-20261002T023822.268791Z-uncommitted` completes: ranked-family source
admission is 5/5 at 8,192 tokens, retaining Rust webhook evidence and admitting
Python metadata, Bash query and both documentation targets. Default retrieval
remains 6/10 and fails its required-target gate on Python metadata. The unchanged
810-file fingerprint publishes in 933.904 seconds; identifier indexing takes
265.390879 seconds. Whole-Sim remains 4/5 for this diagnostic. These results support
continued primary/navigation separation without hard filtering, not production
promotion. Graph-discovered source admission remains the next quality gap.

Primary-first diagnostic whole-Sim run
`context-20261002T022720.997895Z-uncommitted` is terminal: default retrieval stays
3/10 and fails its required-target quality gate, while ranked-family source
admission improves from 3/5 to 4/5 at 8,192 tokens. Python now hits; TypeScript
is selected at depth one but still misses packing. The unchanged 2,572-file
fingerprint publishes in 367.019 seconds; identifier indexing takes 75.295056
seconds. This supports separating primary evidence priority from navigation
occurrences without removing seeds. Production promotion remains unaccepted;
graph-discovered evidence admission and Shardline regression quality remain open.

ADR-0289 adds a diagnostic-only stable primary-evidence-first ordering of the
same bounded family seed set. No reference IDs are filtered and no production
routing changes. All 22 non-ignored MCP tests and strict MCP Clippy pass, including
256 membership/order/idempotence cases. Selected-file run
`context-20261002T022638.121003Z-uncommitted` admits all five target sources at
8,192 tokens. Whole-Sim verification is running; selected-file success is not
general quality acceptance and the previous equal-family rejection remains valid.

Whole-Sim `context-20261002T021240.771653Z-uncommitted` finishes with the
required-target quality failure: default source retrieval remains 3/10 and the
ranked-family diagnostic remains 3/5 at 8,192 tokens. The unchanged 2,572-file
fingerprint publishes in 361.498 seconds; identifier indexing takes 75.317239
seconds. Ranked frontier now selects TypeScript at depth one, but packing still
misses its source; Python is selected at depth zero and still misses packing.
Exact duplicate coalescing retains multi-node attribution but is not enough.
The next quality gap is source admission/allocation, not this TypeScript route;
no equal-family default policy is accepted and no identical rerun is needed.

Artifact diagnosis further separates the admission gap from deduplication:
Python's 15 packed excerpts total 28,485 rendered bytes, with one 16,613-byte
dashboard excerpt accounting for about 58%; only two excerpts have coalesced IDs.
TypeScript's 21 excerpts total 20,383 bytes with one coalesced excerpt, and its
selected depth-one target still has no packed item. Both report no source warnings.
Bytes are not token costs. Investigate ranked seed versus graph-discovered evidence
admission using existing compiler boundaries; do not discard navigation facts or
claim that exact duplicate removal solves allocation.

Full contributor CI for ADR-0288 exact source excerpt coalescing passes in 166.97
seconds, including all 17 backend scenarios and contributor gates. This closes
compiler/backend regression verification, not retrieval quality. Whole-repository
source admission must be measured separately with unchanged fixtures and budgets.

ADR-0288 coalesces exact source excerpt duplicates using the existing multi-node
ContextItem contract, retaining first-ranked position and all contributing IDs.
Different locations, text or evidence classes remain separate; overlaps are not
merged. All 49 Query tests, temporal four-backend/edit/reopen conformance and strict
Query/Turso Clippy pass. Full CI and representative retrieval quality remain open;
this does not change selection or accept the rejected family allocation policy.

Full contributor CI for the expanded ADR-0287 occurrence-specific temporal fixture
passes in 127.05 seconds, including all 17 backend scenarios and contributor gates.
The typed tie-break/current/history/edit/reopen coverage is verified; the older
pending statements below are chronological snapshots, not current blockers.
Cross-family retrieval allocation and representative quality remain open.

ADR-0287 now has occurrence-specific end-to-end packing coverage in the existing
temporal backend fixture. Synthetic source-backed import/export/diagnostic facts
have lower IDs than the definition and equal explicit relevance/distance; the
definition packs first without discarding occurrences. Results match across all
four backends, current/history, source edits and durable reopen. The scenario and
strict Turso Clippy pass. This closes the typed tie-break gap, not parser or
repository quality; full CI for the fixture addition remains pending.

Full contributor CI after ADR-0287's module-occurrence granularity correction
passes in 174.68 seconds, including all 17 backend scenarios and contributor gates.
Explicit classification tests pass, but existing backend context fixtures do not
exercise these three occurrence kinds in a packing tie; that targeted coverage
remains open. Compiler regression safety is not a retrieval-quality claim.

ADR-0287 corrects source granularity for canonical Import, Export and module
resolution diagnostic facts from Local to Occurrence, reusing the existing
classifier. These facts remain available as source evidence and graph navigation;
relevance, distance, storage and producer semantics are unchanged. This affects
only otherwise tied packing candidates, not cross-family allocation. Explicit
classifier fixtures are added; broader contributor/quality verification remains
separate from the previous green ranked-frontier CI.

Full contributor CI for ADR-0286 ranked-frontier semantics and saturated retained
backend conformance passes in 172.54 seconds, including all 17 backend scenarios
and contributor gates. This closes compiler correctness/CI verification for the
opt-in boundary, not representative retrieval or cross-family source packing.
No default host policy or previously rejected allocation is promoted.

Ranked frontier saturation now conforms across InMemory/File/SQLite/Turso: two
seeds competing for one vacancy admit the neighbor of the first-ranked seed
under either order, retain zero neighbor relevance, and produce identical current
and historical packs. Retained packs remain equal after edits and durable reopen.
The extended existing backend scenario and combined Query/Turso strict Clippy
pass. Full contributor CI is running; repository quality and cross-family packing
remain open. No family allocation or default host policy is promoted.

ADR-0286 aligns opt-in ranked-plan bounded frontier admission with caller priority,
reusing the shared expansion loop; ordinary methods remain canonical and neighbors
remain zero-relevance. An adversarial two-seed/one-vacancy fixture verifies priority
versus canonical admission, depth, one hydration and admitted-edge retention.
All 47 Query tests and strict Clippy pass. Saturated current/historical backend
parity, full CI and repository-quality evidence remain open. This is not acceptance
of the rejected equal-family allocation or a fix for Python's packing failure.

Focused Sim TypeScript helper-route diagnostic indexes the unchanged source file
alone and confirms the canonical `Calls` edge. Helper-only expansion reaches
`generateBlockDoc` at depth one under both 64 and 256-node diagnostic bounds;
the former truncates, the latter admits all 114 nodes. This narrows the next
investigation to multi-seed admission/capacity while leaving whole-repository
resolution equivalence unproven. The dedicated ignored Rust test and strict MCP
Clippy pass; no default bounds, edges or host policies change (ADR-0284).

Completed ranked-family artifact diagnosis: Python's desired method is seed 18;
packing admits a 16,613-byte dashboard expression plus multi-kilobyte log spans
and SDK/test evidence before it. Its canonical method span is 2,670 bytes.
Priority survives, but flat cross-family composition ranks costly occurrences
ahead of the method. References must remain available for navigation (earlier
Shardline Rust evidence rejects hard filtering), without treating display-content
matches as equivalent to symbol definitions. TypeScript's helper is seed one
but its target is absent from capped selection; graph admission needs a separate
diagnosis. No new quota, budget increase or default policy is accepted.

Whole-Sim ranked-family `context-20261002T012914.538931Z-uncommitted` is terminal
with the required-target default quality failure and ten measurements (default
3/10). Publication took 370.755 seconds; exact identifier index construction
took 75.134345 seconds. Ranked-family source admission is 3/5 at 8,192 tokens:
JavaScript, Bash and Quickstart hit, while TypeScript misses selection and Python
is selected at depth zero but misses source packing. All five selections reach
64 nodes. The selected-file 5/5 result does not generalize; this equal-channel
allocation is not accepted for defaults. Ranked-plan compiler correctness and
green CI remain valid, but selection and packing are separate remaining quality
failures. Diagnose them without inventing edges or tuning known-target quotas.

Full contributor CI after the ranked-seed Query boundary, constrained-budget
checks, retained-backend conformance and family diagnostic integration passes
in 182.77 seconds. Whole-Sim
`context-20261002T012914.538931Z-uncommitted` remains live and indexing the
unchanged 2,572-file fingerprint with corpus/index/graph/family probes. The
selected-file 5/5 source result is not whole-repository acceptance; default
ranking remains unchanged until representative source-admission gates close.

Selected-file Sim `context-20261002T012757.379648Z-uncommitted` completes with
ranked family-plan source admission 5/5 at the unchanged 8,192-token limit;
TypeScript now hits at 8,149 tokens versus its prior unordered-plan miss. Only
the family probe uses ranked selection/packing; older probes remain unordered.
Subsequent logs explicitly report `packing_priority=ranked_plan` or `unordered`.
Strict MCP Clippy passes. Full CI and whole-Sim ranked-family evaluation are
running; this selected-file result does not authorize default promotion.

Ranked-plan validation now demonstrates complete source admission changing with
plan reversal under the same constrained serialized-byte budget, bounded tokens
and explicit omissions; unknown and over-capacity plans fail explicitly. All 46
Query tests and strict Clippy pass. The existing temporal backend fixture verifies
the same two-seed ranked pack across InMemory/File/SQLite/Turso, after edits and
durable reopen; that scenario and combined Query/Turso Clippy pass. This closes
initial ranked-plan retention evidence, not full CI or repository retrieval.
Diagnostic routing and representative source-admission gates remain open.

ADR-0285's additive ranked-seeded Query selection/packing methods now reuse the
existing compiler, assigning decreasing priority to first-distinct seeds while
neighbors retain zero relevance. Ordinary explicit methods remain unordered.
The shared fixture verifies source ordering, duplicates, reversal invariance of
ordinary methods, current/historical equivalence, exact counts and empty/33-input
bounds; all 46 Query tests and strict Query Clippy pass. Constrained-budget,
durable cross-backend, diagnostic-routing, repository and full-CI gates remain
open. No host policy or default retrieval change is accepted.

Selected-file family prototype's TypeScript miss is packing, not selection:
`generateBlockDoc` is seed seven and selected at depth zero, but its evidence is
absent from the 8,190-token pack. Existing explicit seeds all receive maximum
relevance, erasing the selector's priority before packing. ADR-0285 proposes a
separate ranked-plan Query boundary reusing the compiler and keeping ordinary
unordered methods unchanged. Its priority propagation/public contract and
cross-backend/token/source-admission gates are not yet accepted or implemented.

ADR-0284's test-only globally weighted exact/stemmed family round-robin prototype
now feeds the existing explicit-seed graph/context path under its unchanged
32-seed/64-node/8,192-token ceilings. Competing policies are rejected and the
dedicated family flag is recorded with all other flags. All 21 MCP unit tests,
stdio integration, 15 xtask tests and strict Clippy pass. Selected-file Sim
`context-20261002T012126.493351Z-uncommitted` completes its default matrix, but
family-seeded source admission is only 4/5: TypeScript misses, while Python,
JavaScript, Bash and Quickstart hit. This is not acceptance of the policy;
diagnose that failure before repository-scale promotion. Updated full CI and
whole-repository source admission remain open. No default or public Query change.

Whole-Sim `context-20261002T011024.747981Z-uncommitted` is terminal with ten
default measurements and the required-target quality failure (default 3/10).
Global/family-local stemmed ranks are TypeScript code 35/34, Python code 4/31,
JavaScript code 1/1, Bash code 1/2 and Quickstart documentation 1/1. Family-local
rarity materially regresses Python; retain complete-generation rarity for the
next separated-family prototype rather than promoting this scope change.
This closes the Sim statistics comparison, not family-seeded source admission,
allocation, Shardline quality or a public/default contract (ADR-0284).

Full contributor CI for ADR-0284's family-local/global statistics comparison and
exhaustive refill checks passes in 130.05 seconds, including all 17 backend
conformance scenarios, architecture/extension gates, migrations, strict workspace
Clippy and documentation. Whole-Sim
`context-20261002T011024.747981Z-uncommitted` remains live, with the same 2,572-file
fingerprint and only the corpus diagnostic enabled; no ranking policy is promoted.

Selected-file Sim `context-20261002T010941.275797Z-uncommitted` completes
successfully after adding explicit family-local versus complete-generation
rarity diagnostics. An adversarial source-less fixture proves these scopes
can invert code rankings (ninth versus first); both exact and stemmed views
reuse the existing scorer and retain canonical payload identity. All 21 MCP
unit tests, stdio integration and strict MCP Clippy pass. Default retrieval,
preview selection and public contracts are unchanged; whole-repository scope
comparison and updated full CI remain open (ADR-0284).

Whole-Sim family diagnostic `context-20261002T005340.222311Z-uncommitted`
is terminal: default retrieval remains 3/10 with all ten measurements emitted.
Code-family stemmed target ranks are TypeScript 35, Python 4, JavaScript 1 and
Bash 1; Quickstart ranks first in the documentation family. Python's top code
candidates include rate-limit tests and both SDK languages, not only the desired
Python method. This supports separate families without proving an allocation
or source-admission improvement. The eight-ID round-robin preview remains
unaccepted; its limited code share cannot directly include Python's fourth-ranked
target. ADR-0284 retains these explicit limits before a public Query design.

Full contributor CI for ADR-0284's diagnostic family classification and ordered
duplicate-refill preview passes in 130.96 seconds, including all 17 backend
scenarios and contributor checks. Whole-Sim
`context-20261002T005340.222311Z-uncommitted` is live and indexing to measure
the code/document/reference/other ranks; it does not feed preview IDs into
context selection. No public/default ranking contract is accepted.

Selected-file Sim `context-20261002T005229.051781Z-uncommitted` completes
successfully with the family diagnostics and strict ten-row matrix validation.
The emitted Python report contains all four families and an eight-ID preview.
This closes selected-file report plumbing, not whole-repository relevance or
acceptance of the round-robin preview. Full CI remains pending for these helpers.

ADR-0284 diagnostic helpers now classify canonical code/document/reference/other
families and report separate stemmed target ranks/top candidates. An explicitly
named round-robin preview preserves each input channel's relevance order,
refills duplicate vacancies and exhausts remaining channels under one distinct-ID
cap (maximum 32; preview eight). Dedicated sibling tests cover Script/Module/File,
docs, resolved/unresolved/ambiguous references, external anchors, duplicate-heavy
channels, exhaustion, zero limits, determinism and the global bound. All 19 MCP
tests and strict Clippy pass. The preview does not feed graph/context selection;
no fusion policy, public API, storage change or production ranking is accepted.
Selected-file integration is starting; whole-repository family evidence and
full CI for these helpers remain open.

Whole-Sim diagnostic-only `context-20261002T004040.155335Z-uncommitted`
is terminal with the required-target quality failure and all ten measurements;
default quality remains 3/10. Python stemmed rank improves from 33 overall to
7 among definitions, while Bash improves from 4 to 3. “Definitions” still
includes long documentation chunks; TypeScript's leading definition candidates
are predominantly docs. ADR-0284 records separate code/document/reference
families and order-preserving duplicate refill as the next design direction,
with no accepted allocation weights or public/default contract change. Do not
rerun rejected seed policies or increase graph budgets as a substitute for this
representation/selection diagnosis.

Full contributor CI after the definition/reference diagnostic fields and strict
case-matrix validator passes in 128.78 seconds, including all 17 backend scenarios
and contributor checks. Selected-file Sim
`context-20261002T003737.956043Z-uncommitted` completes successfully with exactly
ten case/budget rows and the new canonical diagnostic fields/definition ranks.
This closes integration verification only. Whole-repository default quality
remains Sim 3/10 and Shardline 6/10; rejected seed policies are not promoted.


Stemmed diagnostic reports now retain bounded top-five canonical kind/identity/
owner/source fields and separate exact/stemmed definition target ranks. They
reuse the existing definition eligibility rule, now shared with the seed selector,
without changing ranking or selection. Regression checks distinguish reference
from definition ranks and verify report identity/source fields. All 17 MCP tests
and strict Clippy pass. This enables diagnosis of occurrence crowding versus
missing vocabulary; no new retrieval policy or quality improvement is claimed.

Whole-Sim mixed-seed run `context-20261002T002107.398443Z-uncommitted`
is terminal with the required-target quality failure. Both ranking oracles
match for all five cases; default retrieval stays 3/10. Mixed 2/4/2 allocation
admits source for only 2/5 targets at 8,192 (JavaScript and Quickstart), regressing
TypeScript relative to path-only 3/5 and still missing Python/Bash. Reject this
allocation for default integration. Python/Bash each deduplicate to four seeds;
Python expansion saturates 64 nodes while Bash finishes at 27 without reaching
its target. Missing targets are therefore not explained solely by a global
graph-node cap. No additional run of this rejected policy is needed. The
cached complete-corpus reference preserves all five exact payload/score checks
without per-case historical database reference scans; full CI already passed
in 130.07 seconds. Production retrieval is unchanged.

Full contributor CI for the mixed-seed diagnostic passes in 130.07 seconds,
including all 17 backend-conformance scenarios and contributor gates. Its larger
fixture verifies that 100 path candidates contribute only two IDs while the
deduplicated result remains capped at eight and deterministic. Whole-Sim
`context-20261002T002107.398443Z-uncommitted` is live, indexing the same 2,572-file
input digest as the rejected path-only baseline; no mixed-policy whole-repository
quality result exists yet. This remains test-only hypothesis verification.

Selected-file Sim mixed-seed run
`context-20261002T002015.338162Z-uncommitted` completes successfully and admits
source evidence for all five targets at 8,192 tokens, preserving both ranking
oracles. This is diagnostic plumbing evidence, not whole-repository acceptance;
the whole-Sim comparison is starting with the cached complete-corpus reference.

A diagnostic-only mixed seed allocation reuses the existing exact/stemmed
all-fact and definition selectors plus the verified path index: at most two
all-fact, four definition and two path slots, deduplicated to at most eight IDs.
It retains reference candidates without exclusively filtering seeds to definitions.
`SYNTAXMESH_CONTEXT_MIXED_SEED_PROBE=1` requires path and graph probes and is
recorded in benchmark metadata; other exclusive channel policies are rejected
by the existing path preconditions. Deterministic composition and empty-query
checks pass with all 17 MCP and 14 xtask tests and strict Clippy. This allocation
is an unaccepted hypothesis, not a production ranking/public-contract change.
Selected-file evaluation is starting; whole-repository admission and full CI
for this diagnostic remain open.

Whole-Sim `context-20261001T235340.822102Z-uncommitted` is terminal with
the required-target quality failure; ingestion and all five index/oracle checks
succeed. Default retrieval remains 3/10: JavaScript only at 8,192 and Quickstart
at both budgets. Exact label-plus-path seeds admit source at 8,192 for 3/5 targets
(TypeScript, JavaScript, Quickstart), missing Python and Bash. Reject this policy
as the default; the opt-in derived index remains valid but is not a sufficient
seed-selection solution. Bash label stemming ranks the target 4th, versus 231st
for exact path enrichment; Python label stemming ranks 33rd, versus outside the
path index's top 256. These are independent diagnostic leads, not accepted fusion
or stemming policies. The complete baseline is retained; use the now-verified
cached complete-corpus oracle for subsequent iteration instead of repeated
database reference scans. No further acceptance run of this rejected policy is
needed. Production retrieval remains unchanged.

Full contributor CI after cached complete-corpus label-oracle reuse passes in
136.83 seconds, including all 17 backend-conformance scenarios and workspace/
doc, strict Clippy, architecture/extension/migration, mixed-feature and dependency
checks. This closes harness integration, not retrieval acceptance. The baseline
whole-Sim run has completed four path-seeded cases: TypeScript and JavaScript
admit source at 8,192; Python and Bash miss. Documentation remains pending.
Do not promote the path-only seed policy based on these partial results.

Selected-file Sim `context-20261002T001155.005987Z-uncommitted` completes
successfully with the cached complete-corpus label oracle, both index-equivalence
checks and all five path-seeded source-admission cases at 8,192 tokens. Metadata
records the enabled corpus/identifier/path/graph flags and disabled alternative
seed flags. This verifies updated diagnostic plumbing, not whole-repository
quality or a measured speedup. The unchanged baseline whole-Sim run now also
admits JavaScript via path-seeded context; Python still misses and two cases
remain pending.

Future sibling diagnostics reuse the existing complete corpus oracle for label
ranking as well as path ranking, avoiding repeated historical database reference
scans after the pinned corpus has loaded. Exact indexed/reference score and
canonical-payload equality and per-node hydration checks remain. A nonempty
Turso fixture compares the cached label oracle against Query's two-pass store
reference across five queries, including empty/no-match and multi-term cases.
All 17 non-ignored MCP tests and strict Clippy pass. Diagnostic output labels
the reference mode `complete_pinned_corpus`; this is harness-only reuse, not a
production query change. The live whole-Sim run continues with its originally
built store-scanning reference binary and digest. Updated diagnostic integration
and timing evidence remain pending.

Partial whole-Sim path-channel evidence from
`context-20261001T235340.822102Z-uncommitted`: the diagnostic corpus contains
87,486 nodes. Both label and path rankings match their complete oracles for the
TypeScript case. Path-ranked seeds reach `generateBlockDoc` at graph depth 1 and
admit source evidence at 8,192 tokens; default retrieval misses both budgets.
The target itself is outside the path channel's top 256, so success comes from
graph expansion, not direct target ranking. Path query time is 42.553 ms versus
144.366 seconds for the label reference scan; this single debug case is not an
isolated latency SLA or like-for-like path/reference performance comparison.
Four cases remain pending and production retrieval is unchanged.

Whole-Sim path-channel run `context-20261001T235340.822102Z-uncommitted`
has published all 2,572 files in 380.381 seconds and remains live in subsequent
index/retrieval diagnostics. Publication succeeds; no completed natural-language
or path-seeded retrieval score is available yet. Debug timing overlaps contributor
CI and is not isolated deployment latency evidence.

Full contributor CI after the path-aware sibling evaluation, Query service
wrapper, complete source-input fingerprint and probe metadata changes passes
in 176.93 seconds. This includes all 17 backend-conformance scenarios,
workspace/doc tests, strict Clippy, architecture/extension/migration checks,
mixed-feature builds, dependency policy and API documentation. Existing allowed
dependency warnings remain. The separate whole-Sim retrieval run is still live;
CI closes integration verification, not whole-repository relevance or speed.

Future context benchmark metadata records all six diagnostic probe flags,
including disabled policies, so path/label/graph seed evaluations are distinguishable
without shell history. All 14 xtask tests and strict Clippy pass. The active
whole-Sim run `context-20261001T235340.822102Z-uncommitted` predates this metadata
addition: it uses corpus, identifier, path-identifier and graph-seed probes, with
definition/all-fact-channel probes absent. Its prepared corpus contains 2,572
files; no completed relevance result is available yet. It retains the source
digest from before this metadata-only change and runs the already built binary.

The existing sibling-repository retrieval evaluation now accepts explicit
`SYNTAXMESH_CONTEXT_PATH_IDENTIFIER_INDEX_PROBE=1` alongside corpus and identifier
probes, rejecting incompatible channel seed policies. It checks path-index
canonical payloads and scores against a complete independently calculated corpus
oracle, preserves label-index/reference checks, and optionally feeds path-ranked
seeds through the existing graph/context diagnostic. Selected-file Sim run
`context-20261001T235239.303575Z-uncommitted` completes successfully, including
the path oracle and graph/context path. All 16 non-ignored MCP tests pass;
strict Clippy passes after test-code arithmetic/ownership cleanup. The run
predates that cleanup. Whole-repository quality remains unmeasured for this
channel; production retrieval is unchanged.

Benchmark source fingerprints now include sorted, length-framed paths and bytes
for the root Cargo/lock/task/toolchain inputs and recursive `crates`, `xtask`,
and optional `.cargo` inputs, excluding nested build/Git output and rejecting
symlinks rather than silently missing their targets. This fixes the previous
nine-file subset that omitted extractor and identifier-index changes. A regression
test verifies nested source edits/renames affect the digest while build output
does not; all 13 xtask tests and strict Clippy pass. This is a local input digest,
not a complete compiler/environment or external dependency build attestation.
The Query path-index service wrapper also passes all 46 Query tests and strict
Clippy; retrieval quality and default integration remain open.

Full CI after ADR-0283 and its durable path-history scenario passes in 169.27
seconds, including all 17 backend-conformance scenarios and contributor checks.
The subsequent restricted-port Query test reuses the existing unexpected-operation
fixture pattern to verify manifest-only checking and selected-ID batch hydration,
with no hydration on posting-budget failure. All 46 Query tests and strict Clippy
pass. This does not bound adapter internals or prove O(1) queries; whole-repository
relevance and default integration remain open. Full CI predates this added test.

ADR-0283's opt-in path-enriched identifier builder is implemented with explicit
file/inventory budgets and retained-generation inventory. All 45 Query tests
and strict Clippy pass, including an independent multi-node ranking oracle,
deduplication, exact budgets, malformed-page checks and InMemory rename history.
The new shared conformance scenario verifies exact rankings after rename/label
edit and durable restart across InMemory/File/SQLite/Turso; focused fixture test
and strict Clippy pass. Full contributor CI is starting with 17 conformance
scenarios. Query-work instrumentation and whole-repository quality remain open;
default context retrieval is unchanged.

Full contributor CI after shared diagnostic corpus reuse passes in 132.94
seconds, including all 16 backend-conformance scenarios, strict Clippy,
workspace/doc tests, mixed-feature builds, architecture/extension/migration
checks, dependency policy and API documentation. This closes refactor
integration verification, not ADR-0283 implementation or retrieval quality.

The test-only corpus probe now loads its immutable pinned node/file corpus once
and shares it across cases; reference ranking and canonical hydration checks
remain per case. All 16 non-ignored MCP tests and strict Clippy pass. Selected-file
Sim `context-20261001T232953.332992Z-uncommitted` completes successfully in
15.87 seconds and exercises the shared corpus/index equivalence path. This is
not a whole-repository speedup or relevance claim. ADR-0283 proposes reusing the
existing generation identifier index for an opt-in path-enriched channel;
implementation, retained-history conformance and whole-repository gates remain
open. Production retrieval is unchanged.

Whole-Shardline run `context-20261001T221721.317496Z-uncommitted` is terminal.
All ten natural-language target/budget measurements and five diagnostic packs
were emitted; exit 105 reports the required Python-target quality failure, not
an ingestion failure. Production scores 6/10: Rust and Bash hit only 8,192,
Python misses both budgets, and both documentation targets hit both budgets.
The all-fact exact/stemmed seed diagnostic scores 3/5 (Rust and both docs),
missing Python and Bash. Reject this policy for production: it restores Rust
relative to the definition-only diagnostic but does not improve the earlier
exact-all-fact diagnostic's 3/5 coverage. No Sim run of this rejected policy is
needed for acceptance. Python path enrichment remains a promising independent
signal, not a proven retrieval fix. Before the next policy comparison, avoid
reloading the identical pinned diagnostic corpus for each case; preserve all
cases, bounds, and the indexed/reference equivalence check. Full CI for this
test-only diagnostic passed in 133.84 seconds; production ranking is unchanged.

Partial whole-Shardline evidence from
`context-20261001T221721.317496Z-uncommitted`: 810 files publish in 950.802
seconds and the identifier index builds in 267.522 seconds. The all-fact
exact/stemmed diagnostic reaches Rust webhook evidence at depth 1 and admits
it at 8,192 tokens, but does not reach Python `load_metadata`. Default Rust
retrieval misses 2,048 and hits 8,192; Python misses both budgets. Python's
test-only label-plus-source-path ranking places the target 2nd versus 163rd
for label-only ranking, supporting a future path-channel test, not acceptance
of the current seed policy. Bash and documentation cases remain pending;
this is not a completed whole-repository score. The run includes generated
Graphify reports. Two brief CPU samples are retained alongside its raw logs;
they are not complete bottleneck attribution or deployment latency evidence.

Full contributor CI after the test-only all-fact exact/stemmed seed diagnostic
passes in 133.84 seconds, including all 16 backend-conformance scenarios,
workspace/doc tests, strict Clippy, architecture/extension/migration checks,
mixed-feature builds, dependency policy and API documentation. This verifies
integration, not retrieval quality. Whole-Shardline diagnostic
`context-20261001T221721.317496Z-uncommitted` remains active; no completed
retrieval score is available. Production ranking is unchanged.

Full CI after Python version 4 and the verified Python/TypeScript/JavaScript
File-store upgrade fixtures passes in 146.18 seconds, including all 16 backend
scenarios and contributor checks. The next diagnostic reuses exact/stemmed
scoring but preserves all fact kinds as eligible seeds, to test the reference
exclusion responsible for the earlier Shardline regression. It is test-only,
full-scan, not accepted for production, and not yet measured on either whole
repository. Production ranking is unchanged.

Source-host producer-upgrade fixtures now cover Python 3→4 and TypeScript/
JavaScript 10→11 through the existing verified File-store Engine path. All
three tests and strict Clippy pass: unchanged input re-extracts, old/new evidence
and stable IDs/call edges survive restart, and StateChronicle history audits.
Legacy evidence is synthetic; archived producer binaries and SQLite/Turso
upgrade scenarios are not covered. Full CI after Python version 4 is starting.

ADR-0282 extends Python Function evidence to existing tree-sitter definition
ranges while preserving identifier-based class evidence and call/import spans.
Semantic producer version 4 invalidates unchanged source. The substantial suite
is now in `extractor/tests.rs`; all 12 Python tests and strict Clippy pass.
Selected-file Sim `context-20261001T221020.276019Z-uncommitted` completes
successfully after this change. Full CI, producer-upgrade history and updated
whole-repository quality remain independent unfinished gates.

Whole-Sim production retrieval `context-20261001T215901.259609Z-uncommitted`
is terminal: all 2,572 supported files publish, but the quality gate fails with
3/10 natural-language target/budget hits, matching the prior valid run's exact
hit outcomes. TypeScript/Python/Bash miss both budgets; JavaScript fits only
8,192; Quickstart fits both. Explicit target seeding admits source evidence for
all five targets at 8,192, including the expanded TypeScript function range.
It is not a default-selection improvement. Debug ingestion takes 381.408 seconds;
two brief CPU samples include Turso page/debug-validation work, not a complete
cost attribution or deployment latency evidence. Full CI ran concurrently.

Full contributor CI after Rust definition evidence, the File-store verified
upgrade fixture and file-backed benchmark logging passes in 180.43 seconds.
It includes all 16 backend scenarios, workspace/doc tests, strict Clippy,
architecture/migration/extension checks, dependency policy and API docs. The
new whole-Sim production run `context-20261001T215901.259609Z-uncommitted` is
still indexing 2,572 supported files; emitted logs are available immediately,
but no retrieval score exists yet. Timings are concurrent, not isolated evidence.

The disk-backed whole-Sim run `context-20261001T212348.920788Z-uncommitted`
and the subsequent Rust-change CI process disappeared before completion. Their
handles/processes are absent; the Sim evidence directory contains only metadata
and prebuild logs, not retrieval samples. Neither run is claimed successful.
The benchmark now redirects child stdout/stderr directly into create-new files
as it runs, instead of retaining all output in memory until exit. Partial logs
remain incomplete evidence, and this is not a power-loss durability guarantee.

ADR-0281 extends Rust Function evidence to full syn definition ranges after
identifier-keyed reference/containment passes. Required trait methods retain
signature/terminator evidence without an invented body. Producer 0.15.0
invalidates unchanged extraction inputs; stable IDs and call ownership remain.
All 31 Rust extractor tests, strict Clippy and 16 Engine unit tests pass. Full
CI and a dedicated retained-history producer-upgrade scenario remain open for
this change; the live Sim run contains no Rust inputs and cannot prove it.

The new Engine File-store upgrade fixture now closes the dedicated reference
scenario: synthetic 0.14.0 identifier evidence survives restart and real 0.15.0
re-extraction of unchanged input, with stable symbol IDs/call edges, exact old/new
ranges and StateChronicle chain audits across restart. Test and strict Clippy
pass. It does not establish SQLite/Turso or archived-binary upgrade coverage;
full contributor CI after this Rust change is running.

Full contributor CI after ADRs 0279–0280 and Shardline-style owned temporary
fixtures passes in 215.08 seconds: architecture/extension/migration checks,
mixed-feature builds, strict Clippy, dependency policy, workspace/doc tests,
all 16 backend-conformance scenarios and API documentation. Whole-Sim retrieval
is concurrently running on explicit disk-backed scratch after the temporary
filesystem quota failure; no updated whole-repository score is available yet.
CI timing is not isolated performance evidence.

ADR-0280 fixes ECMAScript function evidence extent using existing Oxc AST
ranges: named declarations, methods and named arrows now cover their bodies;
call/import occurrences and stable symbol IDs retain their existing semantics.
Producer semantic version 11 invalidates unchanged extraction inputs. All 24
ECMAScript tests and strict Clippy pass. Selected-file Sim evaluation
`context-20261001T212100.939723Z-uncommitted` passes, including TypeScript source
admission at both budgets. This is not whole-repository relevance evidence:
`context-20261001T211650.069911Z-uncommitted` failed during indexing with a
temporary database quota error and produced no retrieval measurements. The
latest valid whole-repository production scores remain Sim 3/10, Shardline 6/10.

ADR-0279's explicit-seed composition boundary is verified in the shared Query
fixture: it preserves the question and exact token accounting without merging
lexical seeds, retains bounded graph expansion, and rejects invalid seed sets.
All 39 Query and 15 non-ignored MCP tests and combined strict Clippy pass. The
test-only sibling probe now preserves its question through this path; its
previous empty-question results have not been remeasured. Production retrieval
policy and the outstanding cross-repository quality gates are unchanged.

Full contributor CI after ADRs 0277–0278 passes in 282.67 seconds, including
the mixed-feature checks and all 16 backend-conformance scenarios. Independent
whole-Shardline `context-20261001T204709.088969Z-uncommitted` preserves production
6/10 target/budget hits and documentation rationale. Reject its diagnostic
exact/stemmed definition-seed policy: only 2/5 targets are retrieved there,
despite 5/5 on Sim. The container packing tie-break is verified separately;
candidate-policy acceptance and answer-bearing body coverage remain open.
These concurrent-run timings are not isolated performance evidence.

The test-only exact/stemmed definition/document seed comparison now returns
source evidence for all five whole-Sim targets within the existing 64-node/
8,192-token bounds (`context-20261001T203802.506431Z-uncommitted`). ADR-0277's
provisional local/occurrence/container tie-break prevents a whole README from
preceding equally relevant, equally distant local evidence. This is not a
production retrieval fix: the existing natural-language selector still returns
3/10 target/budget hits. The full-scan diagnostic candidate scorer is not enabled
in production; independent quality and answer-bearing body coverage remain open.
All 39 Query tests, separate strict Query/MCP Clippy and the three current/
historical backend context scenarios pass.

ADR-0278 repairs feature-unified temporal-page construction: Store-owned pure
constructors initialize optional fields, and instrumented adapters attach their
measured counters. Both Store-only and Query-only instrumentation workspace
checks pass, as does the previously failing combined Query/MCP Clippy check.
`cargo make check-mixed-instrumentation` is now included in contributor CI.
Default counters are unmeasured, not evidence of zero backend work.

Context packing now prefers definition/document evidence over reference
occurrences only after relevance and graph distance tie (ADR-0276), preserving
the existing exact-count/complete-line compiler. Whole-Sim bundle
`context-20261001T201306.336049Z-uncommitted` restores the selected TypeScript
declaration line in the eight-identifier-seed diagnostic; it does not restore
the complete function body or fix natural-language selection. Whole-Sim remains
3/10 natural-language target/budget hits. Whole-Shardline bundle
`context-20261001T201640.085185Z-uncommitted` preserves its prior 6/10 hits,
including source-backed documentation rationale. Python metadata still misses.
All 38 Query tests and current/retained backend context fixtures pass; full
contributor CI passes in 200.55 seconds, including all 16 conformance scenarios.
CI and Shardline ran concurrently: timings are not isolated performance evidence.

Full contributor verification after ADRs 0273–0274 passes in 219.44 seconds,
including all 16 backend-conformance scenarios, strict workspace Clippy,
architecture/migration/extension checks, dependency policy, workspace/doc tests
and API documentation. The measured worktree was unchanged during the run.
Whole-corpus natural-language relevance remains a separate failing gate.

Historical-point plan fix: Turso's planner selected the time-only index for
the outer lookup, omitting fact ID from the seek. ADR-0274 now explicitly uses
the existing identity/time index in both clauses; a shared SQL constant and
plan fixture guard point/batch/evidence paths. Whole-Sim bundle
`context-20261001T193353.113460Z-uncommitted` preserves all five oracle results
and records 1.9–27.7 ms indexed queries versus 0.24–2.68 s before the fix.
This is directional local single-run evidence, not an SLA. Strict Turso Clippy
and retained-node-page/restart conformance pass. Natural-language retrieval
still misses TypeScript/Python/Bash; the default context policy is unchanged.

ADR-0274 adds bounded historical-node batches for explicit candidate hydration.
The initial parameterized IN implementation regressed whole-Sim queries to
0.76–6.98 s and was removed. The retained prepared-point variant resolves the
generation once and reuses one statement; bundle
`context-20261001T154711.669837Z-uncommitted` confirms exact oracle equivalence,
with 0.24–2.68 s queries, effectively the original point-read costs. No speedup
claim is justified. Cross-backend restart conformance and strict Turso Clippy
pass; execution-plan inspection remains before any further tuning.

Whole-Sim identifier-index comparison completes in
`context-20261001T153125.620176Z-uncommitted`: all five indexed candidate sets,
payloads and scores equal the reference. Build takes 9.17 s; indexed queries
take 0.29–4.08 s versus reference 19.14–21.58 s in this single local sequential
run. These are directional costs, not deployment latency guarantees. Selected
node hydration still needs inspection. Natural-language TypeScript/Python/Bash
acceptance remains failing; default production context ranking is unchanged.

ADR-0273 adds an explicit immutable generation-pinned identifier-postings
channel, reusing normalization and the reference rarity helper. Construction
and matching-posting visits have independent fail-closed count budgets; selected
payloads are hydrated through the pinned store. Repository/worktree/root identity
is checked even for queries with no matches. The expanded retained-node-page
conformance fixture passes across InMemory/File/SQLite/Turso, including restart
and cold-copy verification. Exact-token production context integration is not
enabled: existing whole-corpus relevance failures remain. Logical retained-payload
build budgeting now follows Shardline's cache principle; allocator/transient-memory
budgets, incremental maintenance and durable index persistence remain open.
The full gate before the payload follow-up passed in 217.57 seconds. After it,
all 37 query tests, strict Query/Turso Clippy and expanded node-page conformance pass.

Whole-Sim label-plus-owner-path diagnostic completed in
`context-20261001T151010.606860Z-uncommitted`. Exact/stemmed target ranks are
TypeScript 269/449, Python 1,131/70, JavaScript 1/1, Bash 231/22,
Quickstart 3/1. Compared with label-only ranks, indiscriminate path enrichment
regresses TypeScript and Python beyond the 64-candidate bound. Reject this as a
production policy; source paths would require independently scored fields or
file-level candidate selection, not concatenation into every owned node name.
Production retrieval remains unchanged and its acceptance gate still fails.

Zero-offset exact/stemmed fusion diagnostic completed in
`context-20261001T150313.984749Z-uncommitted`: TypeScript 73, Python 56,
JavaScript 1, Bash 8, Quickstart 2. Reject it because TypeScript remains beyond
64 candidates. Stop offset tuning; richer candidate fields/graph signals and
an explicit indexed retrieval boundary remain to assess. Production scoring
and limits are unchanged. Diagnostic top-label output is now capped at 240
characters consistently, avoiding unbounded expression-label logging.

Candidate-selection isolation: the test-only explicit-target-seed probe uses
the existing MCP context API without relaxing natural-language acceptance.
Whole-Sim bundle `context-20261001T144912.124844Z-uncommitted` returns source
evidence for all five explicit targets within the pinned generation/8,192-token
budget, while natural-language retrieval still misses TypeScript/Python/Bash.
This demonstrates an actionable candidate-selection failure for these cases,
not universal packing correctness or a fix. Strict MCP Clippy and all 14 normal
MCP unit tests pass. Independent Shardline seed bundle
`context-20261001T145109.954228Z-uncommitted` likewise retrieves all five explicit
targets while natural-language Python metadata remains missing. ADR-0272 adds
distinct lookup-saturation (completeness unknown) and merged-seed-truncation
warnings without changing scores/caps. Targeted warning fixtures, strict query
Clippy and shared context backend/restart conformance pass. Whole-Sim warning
rerun `context-20261001T145543.794807Z-uncommitted` preserves the same 3/10
natural-language hits and all five successful explicit-seed probes.
The complete contributor gate after these warnings/probes passes in 225.95
seconds, including all 16 backend-conformance scenarios. Retrieval quality remains
a separate failing acceptance gate.

Turso historical node-page/search schema decoding now uses a single-entry
generation/content-digest cache (ADR-0271), reusing Shardline's bounded,
content-keyed cache principle. Every read still fetches/hashes authoritative
history bytes; misses fully decode, failures are not cached, and a warmed
database-corruption fixture fails closed after bytes change. All 40 Turso unit
tests and targeted historical-page conformance pass. Whole-Sim rerun
`context-20261001T144204.770892Z-uncommitted` preserves all diagnostic ranks and
production misses. Its 132.36-second duration versus an earlier 206.28 seconds
is only directional single-run evidence, with concurrent CI and uncontrolled
cache/load effects. The corrected `cargo make ci` completed successfully in
200.38 seconds, including all 16 backend-conformance scenarios and documentation.

Independent whole-Shardline diagnostic completed in
`context-20261001T141928.826232Z-uncommitted`, 206,395 nodes. All five diagnostic
targets fit 64 candidates, but unchanged production context misses Python
metadata. This does not resolve Sim's regressions or packing/expansion acceptance.
The dev-only Tantivy BM25 experiment was removed after dependency policy found
RUSTSEC-2026-0253 in its incompatible transitive lru version; no advisory exception
was added. Raw results/build provenance remain retained. Dependency policy,
targeted corpus-probe tests, strict MCP Clippy and formatting pass after removal.

Whole-Sim length-normalization diagnostic completed in
`context-20261001T141402.544102Z-uncommitted`: Tantivy unique-term BM25
exact/stemmed ranks are TypeScript 12/34, Python 272/103, JavaScript 12/12,
Bash 171/3, Quickstart 1/1. Reject a blanket replacement: TypeScript improves
but Python regresses beyond 64 candidates. This remains dev-only scoring,
not a production index or relevance acceptance; production retrieval is unchanged.

Whole-Sim exact/stemmed fusion comparison completed in
`context-20261001T140601.449126Z-uncommitted`. Equal-weight reciprocal fusion
produces ranks TypeScript 82, Python 55, JavaScript 1, Bash 24, Quickstart 2;
reject it as the production policy because TypeScript regresses outside the
64-candidate limit. No production behavior changed. The initial attempt hit a
user tmpfs quota before ranking; the successful diagnostic retry used a dedicated
disk-backed TMPDIR. The unchanged production acceptance still misses TypeScript,
Python and Bash. Label-length/field effects and answer-bearing context coverage
remain to investigate; candidate-rank improvements alone are not release proof.

Full contributor verification (2026-10-01): `cargo make ci` completed successfully
in 209.57 seconds after the identifier-normalization, generation-term-statistics,
reference-ranking, Snowball diagnostic and daemon configuration-input changes.
The unchanged worktree passed formatting, architecture boundaries, independent
extension fixtures, migration registries, locked all-feature compilation, strict
workspace Clippy, dependency policy, workspace tests/doc-tests and API documentation.
All 16 backend-conformance scenarios passed, including the expanded historical
node-page statistics/restart/cold-copy fixture. Opt-in live semantic-provider and
whole-corpus retrieval evaluations are not release-quality proof from this gate;
the separately measured production retrieval failure below remains open.

Whole-Sim Snowball comparison completed in bundle
`context-20261001T134829.609633Z-uncommitted`: exact/stemmed ranks TypeScript
46/242, Python 222/33, JavaScript 1/1, Bash 182/4, Quickstart 3/1. Unconditional
stemming is rejected as the production replacement despite Python/Bash gains;
the TypeScript regression requires independent exact-channel preservation and
reviewed merging. No production retrieval policy changed. Context packing,
graph expansion, cross-repository quality and strict production acceptance remain
separate gates. Native shared-index scores also remain unsuitable for pinned
ranking as recorded in ADR-0264.

Identifier normalization allocation follow-up: the shared normalizer now uses
the standard peekable character iterator instead of collecting a full character
vector. Output rules/revision are unchanged. Differential tests compare the
previous algorithm over 512 three-character combinations (case, numeric,
punctuation, Unicode/combining characters) and a large repeated expression label.
All 36 query tests and strict Clippy pass. This removes the auxiliary full-label
character allocation; output term strings remain allocated and no measured
end-to-end speedup or retrieval-quality improvement is claimed.

Whole-Sim normalized-reference result: completed bundle
`context-20261001T133737.880105Z-uncommitted` reports target ranks TypeScript 46,
Python 222, JavaScript 1, Bash 182, Quickstart 3. The exact-token reference is not
accepted as production ranking: Python remains beyond 64 candidates and Bash
regresses versus substring matching. The overall strict production retrieval
evaluation still fails; no context policy changed. A preceding prebuild failed
on an evaluator macro-scope error and provides no retrieval evidence. Corrected
Query/MCP strict Clippy passes. Inspect boundary/morphology handling before
committing to an index representation. Repeated full history-entry decoding per
Turso node page was also identified by code inspection, with one debugger sample;
profiling/optimization evidence remains separate from relevance acceptance.

Reference ranked candidates (ADR-0270): a pure generation-pinned reader now
combines complete normalized document frequencies with integer rarity and term
coverage, retaining at most 256 candidates with stable NodeId ties. It rejects
incomplete statistics rather than pretending truncated counts are exact. The
existing 1,002-node temporal fixture verifies bounded top results, deterministic
ties, partial-statistics rejection and unchanged historical ranking after later
publication. All 35 query tests and strict query Clippy pass. This two-pass
reference scan is for validating derived indexes, not the default context path
or a whole-repository performance/relevance claim. Indexed production integration
and representative whole-corpus ranking evaluation remain required.

Term-statistics durable equivalence: the existing node-page conformance fixture
now compares complete and budget-limited normalized counts against retained
snapshot oracles before/after updates and full deletion. InMemory, File, SQLite
and Turso pass; all retained generations also pass after durable reopening and
cold copies of File/SQLite/Turso, reusing the existing database-set copy helper.
The expanded targeted scenario and strict Turso all-target Clippy pass. This
closes the collector's backend/restart evidence gap, not indexed ranking or
whole-repository relevance. Reference snapshot costs remain distinct from the
collector's bounded page retention.

Generation-scoped term statistics (ADR-0269) now reuse historical-node pages and
normalization v1, with an explicit scan budget and completion flag. Incomplete
counts are lower bounds and must not be used as exact ranking statistics. A
1,002-node two-generation fixture verifies multi-page counts, duplicate terms
counted once per node, unchanged retained statistics after label replacement,
budget truncation, and rejection of excessive terms/unknown generations. The
targeted fixture and strict query Clippy pass. This collector is not integrated
into context requests and is not a claim that scanning is an efficient per-query
ranking index. Durable-backend/restart equivalence remains required.

Candidate-index prerequisite (ADR-0268): the existing context identifier
normalizer is now a shared pure query utility with an explicit revision for
derived-index identity. Context ranking calls the same implementation; no
ranking/search behavior changes. CamelCase, acronym, Unicode and snake_case
fixtures and all 34 query tests pass, as does strict query Clippy. This removes
the need for a second tokenizer in candidate adapters, but does not implement
the generation-scoped candidate index or fix whole-corpus retrieval.

Live-owner resolver-input evidence: the existing daemon fixture now creates and
deletes previously missing package.json and tsconfig.json inputs without source
or SyntaxMesh-policy edits. Both native-watch and polling modes update import
resolution, retain earlier historical answers, and pass a retained StateChronicle
audit after shutdown. All 14 daemon lifecycle tests, including the four new
live-owner scenarios, strict daemon Clippy and formatting pass. This closes the specific missing-input creation/deletion and live-owner
evidence gaps listed below. Package conditional exports, outside-root dependency
notifications, and discovery nonconvergence remain separate open coverage.

Fresh full-gate evidence after ADRs 0265–0267 and the package/input-stability
fixtures: `cargo make ci` passes in 224.23 seconds. It covers formatting,
architecture boundaries, independent extension execution, SQLite/Turso migration
registries, all-feature workspace compilation and strict Clippy, workspace tests
and doctests, all 16 backend-conformance scenarios, and API documentation. No
source edits were made during the run. This validates the integrated changes,
not whole-repository context relevance: the manual whole-Sim retrieval gate still
fails as documented below, and real-provider evaluation remains opt-in.

Resolver-input follow-up evidence: the shared CLI process fixture now also tests
package.json imports-map-only edits on File and verified Turso. All four config
and package scenarios pass restart, changed resolution, retained history and
post-edit idempotence checks. A deterministic provider mutates a known input
during preparation: reconciliation rejects the candidate, preserves the accepted
graph, and later repair publishes successfully with a verified-history audit.
All 13 source-host tests pass. This closes those specific package/rejection
evidence gaps; package exports conditions, missing-config creation/deletion,
discovery nonconvergence and daemon config-only process evidence still need
coverage. No production parser, database, or workflow was added in this follow-up.

CLI/shared reconciliation convergence (ADR-0267): the CLI now passes its existing
scanned inventory to the same resolver-input reconciliation implementation as
the directory/daemon host, retaining scan metrics and its separate semantic
enrichment phase. File and explicitly migrated verified-Turso process fixtures
both pass unchanged restart, extended-tsconfig-only edit, historical retention,
and post-edit no-op checks. The full CLI test suite passes (its real-provider
evaluation remains intentionally ignored), along with strict CLI/source-host
Clippy and all 12 source-host tests. The full-suite run caught an effective-status
reporting regression; reporting now reads the current manifest rather than the
historical canonical manifest, and both verification/host-equivalence regressions
pass on rerun. This supersedes the earlier CLI-path limitation below; package-only
and unstable-input rejection evidence remain open. Verified status is checked
on these Turso generations; this fixture does not perform a full retained-chain
StateChronicle audit.

Shared resolver-input reconciliation (ADR-0266) now fingerprints persisted and
live dependency coverage before planning. Newly discovered paths discard the
candidate and trigger bounded re-preparation; changed known inputs fail before
publication. Coverage is retained across provider replacement and persisted
before acceptance. A real Oxc fixture changes only an arbitrarily named extended
tsconfig, proves current resolution changes, retains the old historical graph,
checks unchanged provider-replacement and post-edit no-ops, and audits verified
history. All 28 Engine/source-host tests, strict Clippy, and all 13 existing daemon
lifecycle tests pass. Those process tests do not yet exercise configuration-only
dependency invalidation. This applies to
the shared reconciler used by the daemon, not yet the CLI's separate indexing
path. Process/disk restart, package-only edits, unstable-input rejection fixtures,
and full workspace acceptance remain follow-up work. Sampling is not an atomic
filesystem snapshot and does not detect adversarial ABA changes.

Pre-publication source preparation (ADR-0265): the Engine now exposes its existing
Indexer preparation step independently from acceptance. Hosts can discard a
candidate after inspecting resolver coverage, or publish through the existing
lineage/Penelope/StateChronicle boundary. `index` retains its convenience behavior
by delegating to the same preparation method. All 16 Engine unit tests pass,
including discard/reprepare/verified-publication coverage; strict Engine Clippy
passes. This is an enabling boundary, not completed automatic resolver-input
invalidation or an atomic filesystem snapshot. Host discovery convergence and
generation-bound input validation remain required before that gap is closed.

Whole-corpus retrieval evidence work: the existing evaluator now supports
`SYNTAXMESH_CONTEXT_EVAL_SCOPE=repository` and records the scope/full input
fingerprint, preserving selected-file mode by default. Expected target selection
now requires exact source FileId in addition to name/kind. The corrected
selected-file Sim test passes all 10 target/budget checks; strict MCP/xtask Clippy
passes. The first whole-Sim debug and release attempts failed on `/tmp` quota,
with no retrieval acceptance result. Raw bundles are
`context-20261001T123052.077053Z-uncommitted` and
`context-20261001T123605.722205Z-uncommitted`. A corrected release run using
workspace scratch completed ingestion of all 2,572 supported Sim files in
16,312 ms, then failed the first case's required 8,192-token target retrieval.
The expected `generateBlockDoc` source node was absent; generic `Page` definitions
were lexical seeds. Raw bundle: `context-20261001T123738.356345Z-uncommitted`.
This is a demonstrated whole-corpus ranking/selection gap, not a latency SLA or
proof of overall semantic answer absence. The run stopped before later cases.
The evaluator reuses the shared host extractor pack.
The complete baseline (`context-20261001T124000.686476Z-uncommitted`) retrieved
3/10 strict targets; TypeScript/Python/Bash missed at 8,192. ADR-0263's attempted
single-term bonus restriction reduced retrieval to 1/10 and lost Quickstart
(`context-20261001T124150.489162Z-uncommitted`). The scoring experiment is rejected
and prior production ranking restored; all 33 query tests and strict query
Clippy pass. Candidate lookup coverage/truncation needs investigation before
further ranking changes. These single-run fixed-case results do not establish
broader semantic recall or deployment latency.

Whole-Sim lookup diagnosis (`context-20261001T124526.759558Z-uncommitted`):
TypeScript's expected node is absent from every 256-result term page; the
`generate` lookup returns it only at the 1,024 cap (684 total matches). Python's
target already appears among 46 `retry` matches. Bash's file target appears in
both `guardrail` (35) and `setup` (218) lookup results. Therefore TypeScript has
demonstrated candidate truncation, while Python/Bash have downstream selection
or packing misses. Do not claim a larger search cap alone solves retrieval.
Diagnostics make additional reads before timed context queries, so this run's
timing is not comparable to uncached measurements. The test helper is now shared
and measures each evaluator's actual candidate limit; strict MCP Clippy passes.
Production lookup/ranking remains unchanged after the rejected scoring trial.
A brief detached debugger sample found the worker in Turso insertion-time cell
debug validation; one sample is not bottleneck attribution. The runner now
accepts explicit debug/release profiles and records them. Optimized whole-corpus
results and reviewed broader relevance criteria remain open.

Generation-bound resolver refresh (ADR-0258) now reuses Oxc's upstream
`clear_cache` behind a lock shared with resolution calls. The indexer invokes
the pure provider refresh boundary before delta preparation. Targeted adapter,
resolver, and indexer tests and strict Clippy pass. A native/polling daemon
process regression adds, deletes, and restores an imported TypeScript target
without changing configuration or caller: current resolution follows each
transition, historical resolution remains unchanged, and StateChronicle audits
after shutdown. Full workspace CI passes in 243.58 seconds, including all 13
daemon lifecycle tests and all 16 backend-conformance scenarios. Subsequent
composite-order and indexer refresh-rejection tests pass targeted tests and
strict Clippy; these later additions are not claimed covered by the earlier
full-gate formatting/Clippy phases. Package and
tsconfig-only inventory invalidation remains open.

Resolver-input investigation: pinned Oxc 11.24.3 exposes existing/missing path
dependencies through `resolve_with_context`, but that directory API does not
perform automatic tsconfig discovery. A targeted native-filesystem regression
proves the current file API resolves a tsconfig path alias while the context API
with no explicitly supplied config rejects it. The test and strict ECMAScript
Clippy pass. Do not replace `resolve_file` with that API as an invalidation fix;
tracked-input integration must preserve automatic discovery and extended configs.

Filesystem-boundary characterization now verifies that a delegating Oxc
`FileSystem` observes both the automatically discovered `tsconfig.json` and its
arbitrarily named `base.json` extension while preserving the existing path-alias
result. The targeted test and strict ECMAScript Clippy pass. This supplies a
viable observation boundary, not production invalidation: missing-path probes,
package reads, restart-persisted dependency inventory, and pre-planning content
comparison still need implementation. A fixed filename scan would miss extended
configs, and an in-memory-only dependency list would lose coverage on restart.

Earlier long-lived resolver gap: a native-filesystem Oxc characterization fixture
resolves a missing `./added` import, creates `added.ts`, and confirms the same
adapter retains the negative lookup while a freshly constructed adapter resolves
the target. The test and strict ECMAScript adapter Clippy pass. This documents
the existing cache lifetime, not a product fix or a new full-CI result. The
daemon replaces providers on project-policy changes but retains them between
ordinary source passes, so generation-bound cache refresh needs implementation
and end-to-end target-addition coverage. Oxc already exposes `clear_cache`, but
its pinned implementation requires no concurrent resolution operations while
clearing. Reuse that capability with an explicit synchronized boundary rather
than copying its cache or clearing unsafely from host code. Package/tsconfig
changes absent from the source inventory are an additional planning gap.

Daemon project policy reload (ADRs 0256/0257) now uses existing config parsing,
provider setup, and reconciliation. Resolver provider/fingerprint replacement
is serialized under the same Engine lock; no store reopening or second parser
is introduced. The native/polling process fixture verifies unchanged TypeScript
imports gain resolution after Node-policy enable, lose it after config deletion,
and retain old resolution in the configured historical generation. Malformed
configuration fails closed and releases ownership. All 11 daemon process tests
and strict daemon Clippy pass. An additional native/polling fixture now covers
verification-policy enable/disable without a CLI override: project policy starts
Verified, disabling plus a source edit publishes Durable, and reenabling after
that gap causes the next canonical publication to record VerificationFailed and
stop serving. Earlier verified-chain evidence still audits; historical canonical
manifests remain Durable rather than being retroactively relabeled. The targeted
transition regression and strict daemon Clippy pass. A fresh full gate including
it passes in 171.06 seconds, with all 12 daemon process tests and all 16 backend
conformance scenarios. Reload's
positive CLI verification precedence passes a retained StateChronicle audit.
Full CI passes in 216.47 seconds, including strict workspace Clippy, all 11
daemon process tests, and all 16 backend-conformance scenarios.
Earlier “config loaded once” notes below describe
the preceding slice, not current project-policy behavior; semantic-provider
reload and broader orchestration remain open.

Earlier full gate: `cargo make ci` passed in 218.04 seconds with the malformed
reserved-reference metadata InMemory/File-restart regressions and test-only
retrieval-miss diagnostics. All 14 indexer tests, strict workspace Clippy,
independent extensions, migration registries, and all 16 backend-conformance
scenarios pass. File rejection preserves exact snapshot bytes, retained facts,
and generation history. Broader product/relevance gates remain open.



Rust producer 0.14.0 additionally pre-registers direct block-local const/static
value bindings with the existing scope frames (ADR-0255), preventing same-name
call guesses before or after these hoisted item declarations. All 30 extractor
tests and strict extractor Clippy pass. Expanded File/verified-Turso lifecycle
fixtures cover the hoisted const calls alongside parameter constraints. Full CI
for this follow-up passes in 213.86 seconds, including strict workspace Clippy
and all 16 backend-conformance scenarios. The manual representative retrieval
evaluations remain ignored by CI; broader relevance evidence remains open.
Module-level bindings and callable-value inference remain open.

Rust value-binding calls now retain explicit unresolved constraints rather than
binding a same-named unrelated function (ADRs 0253/0254). The pure language SDK
encodes non-default constraints in existing occurrence metadata; unchanged-source
re-resolution decodes them and malformed reserved payloads fail closed. Rust
producer 0.13.0 tracks parameter/local/closure/loop/match/conditional pattern
scopes while preserving initializer ordering and qualified-call policy. All 29
Rust extractor tests, 20 SDK/resolver tests, targeted strict Clippy, and the
all-feature workspace check pass. File and verified-Turso lifecycle tests cover
unchanged caller re-resolution after definition edits, restart, cached rescans,
and retained history. Full CI passes in 230.15 seconds, including strict workspace
Clippy and all 16 backend-conformance scenarios. Compiler-grade bindings, nested
item/module resolution, macro expansion, and callable-value inference are open.

Rust impl identities and containment now reuse containing-declaration paths and
actual evidence-span node lookup
([ADR-0252](adr/0252-rust-scoped-implementation-ownership.md)). Local impl IDs are
independent of same-header impls in differently named functions/methods/defaults;
containment uses the nearest emitted declaration rather than always the file.
Unmodeled named contexts remain explicitly ownerless instead of inventing an
outer containment edge. Rust producer 0.12.0 invalidates current extraction;
old generations remain unchanged. All 27 extractor tests and extractor Clippy
pass. New File/verified-Turso lifecycle fixtures cover unrelated insertion,
stable scoped IDs, call edits, caching, restart, and retained history. All 14
shared lifecycle tests and full CI pass in 221.63 seconds, including strict
workspace Clippy and all 16 backend-conformance scenarios. Duplicate ordinals
within one identical path, block scope, lexical
binding, and trait dispatch remain separate open work.

Prepared Penelope record encoding now uses borrowed Serialize-only wire views
instead of cloning full graph deltas, lineage, consequences, events, and manifests
([ADR-0251](adr/0251-borrowed-prepared-penelope-encoding.md)). Owned legacy
decoders, exact bincode field order/bytes, journal schemas, CAS, and recovery
contracts remain unchanged. The existing adapter suite now lives in dedicated
`adapter/tests.rs`. All 22 integration tests and strict adapter Clippy pass;
full CI passes in 163.65 seconds, including workspace strict Clippy and all 16
backend-conformance scenarios. This removes an avoidable allocation, not the
measured publication bottleneck as a whole; representative speedup is not
demonstrated by the follow-up below.

ADR-0251 representative follow-up (2026-10-01T10:32Z, same Rust/toolchain/host and
696-file Shardline fixture): three fresh runs per SQLite/Turso backend passed
all initial/incremental graph-root checks and retained identical fact counts
and database bytes. Median initial/incremental times were 35.539 s / 664 ms for
SQLite and 45.916 s / 2.861 s for Turso; peak RSS was 2,271.3 / 3,037.7 MiB.
Total timings rose versus the prior baseline while RSS changed negligibly.
Prepared-record CAS stage medians moved in opposite directions across backends.
This does not demonstrate an end-to-end latency or memory win, nor isolate a
regression caused by the encoder. Retain the byte-compatible no-copy encoder,
but do not pursue further encoder micro-tuning based on these measurements.
Evidence: ignored
`target/benchmark-results/repository-20261001T103234.385425Z-uncommitted/`.

Rust declaration duplicate ordinals now reuse an extraction-local ordered counter
map rather than scanning every prior node per declaration
([ADR-0250](adr/0250-indexed-rust-declaration-ordinals.md)). Recursion shares the
same map and preserves exact legacy kind/name/ordinal IDs; no producer revision
or storage migration changes. All 24 extractor tests and strict extractor Clippy
pass, including a differential old-scan identity oracle and a 2,000-declaration
fixture. All 12 File/verified-Turso lifecycle tests and full CI pass in 230.47
seconds, including strict Clippy and all 16 backend-conformance scenarios. The
counter map is dropped after collection. This removes one quadratic scan, not
an end-to-end ingestion performance guarantee.

Current-state Shardline ingestion evidence after ADR-0250 (2026-09-30T23:05Z,
rustc 1.98.1, Linux x86_64, Ryzen 9 7950X): the existing evidence runner completed
three fresh SQLite and three fresh Turso runs over 696 Rust files / 14,669,297
source bytes. All initial/incremental generations passed full graph-root checks;
all six post-edit graphs contain 190,831 nodes / 215,968 edges. SQLite median
initial/one-file incremental indexing is 33.354 s / 644 ms; Turso is 42.133 s /
2.422 s. Median retained bytes are 1,030,746,112 / 1,206,841,448, and peak RSS is
2,273.5 / 3,038.9 MiB. Initial delta preparation is 1.195 / 1.212 s, versus
32.151 / 40.813 s Penelope publication: publication dominates this workload.
The runner modifies only in-memory source copies. Raw evidence is in ignored
`target/benchmark-results/repository-20260930T230552.006505Z-uncommitted/`.
Recent extraction/resolution changes altered graph counts, so this is not an
isolated collector speedup comparison, compiler-quality evidence, an SLA, or a
write-amplification result. Broader repositories and lexical correctness remain
open.

Rust imports inside inline modules now use the actual source-backed declaration
identity instead of a second module-path/ordinal reconstruction
([ADR-0249](adr/0249-rust-import-module-source-ownership.md)). This fixes owners
for same-named modules nested inside functions, impl methods, and trait defaults.
The existing declaration-span lookup is shared; Rust producer 0.11.0 invalidates
caches. All 22 extractor tests, extractor strict Clippy, and 12 shared
File/verified-Turso lifecycle tests pass. Full CI passes in 290.66 seconds,
including strict Clippy and all 16 backend-conformance scenarios. Rust lexical
import binding and compiler-assisted resolution remain open.

Local Rust functions now have explicit source-backed `Contains` edges from their
immediate function, impl-method, or default trait-method owner
([ADR-0248](adr/0248-rust-local-function-containment.md)). This reuses the shallow
local-item visitor, span-based identity lookup, and existing graph relation.
Rust producer 0.10.0 invalidates cached extraction. Local module/impl/trait
boundaries are not flattened into false outer-function containment. All 21 Rust
extractor tests and 10 File/verified-Turso lifecycle tests pass. Full CI passes in
215.88 seconds, including strict Clippy and all 16 backend-conformance scenarios.
Lexical target binding and broader declaration containment remain open.

Local Rust declarations now reuse the existing declaration/reference collectors
inside function, impl-method, and trait-default bodies
([ADR-0247](adr/0247-rust-local-declaration-call-ownership.md)). Calls stop at item
boundaries, so a nested function's calls belong to that function rather than its
outer declaration. Names include the containing declaration path; span-based
owner selection remains intact. Rust producer version 0.9.0 invalidates caches.
This is source ownership, not lexical target binding; closure-node identities,
block scopes, macro expansion, and local constant initializer calls remain open.
Full CI is verified by the subsequent ADR-0248 run in 215.88 seconds, including
strict Clippy and all 16 backend-conformance scenarios.

Rust impl blocks now emit source-backed external nodes, file/method containment,
and positive trait implementation occurrences
([ADR-0246](adr/0246-rust-implementation-block-source-facts.md)). The existing
trait-only resolver can publish impl-block-to-trait edges. Inherent and negative
impls emit no positive implementation references. IDs ignore body edits and byte
offsets; call ownership now uses declaration evidence spans, keeping same-named
methods in different impls distinct. Rust producer version 0.8.0 invalidates caches.
File and verified-Turso fixtures cover no-op repetition, positive-to-inherent
retraction, and retained history. Concrete type links, trait dispatch, full module
resolution, and nested declaration completeness remain open. Full CI passes in
213.81 seconds, including strict Clippy and all 16 backend-conformance scenarios.

The existing `Implements` relation now resolves only to trait declarations,
using a relation-specific view of the existing normalized name index
([ADR-0245](adr/0245-trait-only-implementation-resolution.md)). Traits participate
in candidate discovery and edit invalidation but cannot become callable targets.
Resolver revision v3 invalidates shared source-host no-op planning. Focused
resolver tests cover kind filtering, ambiguity, and qualified misses; a synthetic
SDK producer verifies unchanged implementation occurrences rebind/retract after
trait edits while old graph history survives. This is resolver support, not Rust
`impl` fact emission or dispatch inference. Full CI passes in 234.48 seconds,
including strict Clippy and all 16 backend-conformance scenarios.

Rust trait methods now have trait-qualified declaration nodes, including required
methods, and default-body calls are owned by their method nodes
([ADR-0244](adr/0244-rust-trait-method-source-facts.md)). This reuses the existing
parser, declaration identities/spans, call visitor, and resolver pipeline; trait
traversal is a focused sibling module. Default-body `self` remains an unknown
receiver, not inferred dispatch. Rust producer version 0.7.0 invalidates caches.
All 15 extractor tests and six shared File/verified-Turso call-path lifecycle
fixtures pass. Trait dispatch, implementation relationships, associated
constants/types, and cross-file module resolution remain open. Full CI passes in
223.48 seconds, including strict Clippy and all 16 backend-conformance scenarios.

Rust now reuses Python's conservative unknown-receiver policy
([ADR-0243](adr/0243-preserve-unknown-rust-method-receivers.md)). `client.build()`
retains its receiver instead of becoming bare `build` and binding an unrelated
function. Syntactic `self` inside an impl retains the existing type-qualified
match, including parenthesized/reference wrappers. Method-call spans include the
receiver and arguments. Rust producer version 0.6.0 invalidates extraction and
configuration caches. Four shared CLI fixtures cover call-path/receiver edits,
no-op repetition, and retained history on File and verified Turso. Receiver type
inference and compiler/trait dispatch remain open. Full CI passes in 224.68
seconds, including strict Clippy and all 16 backend-conformance scenarios.


Unmatched `::`-qualified references no longer bind unrelated unique terminal
names ([ADR-0241](adr/0241-qualified-resolution-without-terminal-guessing.md)).
Exact normalized and unqualified matching remain intact. Shared source hosts
include the static resolver revision even without module profiles, invalidating
unchanged-source no-op planning once on upgrade. This prevents invented edges;
lexical/module/import/compiler resolution remains open.
Full CI for this resolver-policy change passes in 191.09 seconds. Host-equivalence
and live-watch fixtures use the shared resolver configuration identity; they do
not claim unsupported cross-file Rust module resolution.

`Query::cyclic_components` and `cycles[-turso]` now reuse the retained projection
analysis ([ADR-0240](adr/0240-query-and-cli-cycle-analysis.md)). A real Rust source
fixture verifies matching File/Turso call cycles, current retraction, retained
history, relation filtering, and argument validation. Turso uses the existing
lease; this command is not yet daemon-attached. Architecture rules and broader
cycle-quality/corpus evidence remain open.
Full CI for Query/CLI cycle analysis passes in 212.63 seconds.

The existing generation-tagged projection now offers deterministic directed
cyclic components with optional exact relation filtering, reusing crates.io
petgraph rather than introducing a private SCC algorithm
([ADR-0238](adr/0238-projection-cycle-analysis.md)). Self-loops count as cycles;
selected dangling endpoints fail closed. The SCC kernel is O(V+E), with additional
ordered-map checks and deterministic sorting; this is projection analysis,
not bounded query/CLI integration or architecture-rule completion.
`GenerationGraph::load_retained` now reuses the existing historical snapshot
port for complete retained projections ([ADR-0239](adr/0239-retained-generation-projections.md)).
A fixture removes a cycle edge in a later generation and verifies old cyclic
components remain unchanged while the current graph becomes acyclic. Current-only
projection loading is unchanged; full materialization remains output-sized.
Full CI for retained projection loading passes in 168.69 seconds.

`Query::fact_history_page` now delegates to the existing generation-pinned store
history-page contract with a 1–1000 item limit
([ADR-0236](adr/0236-query-fact-history-pages.md)). It reuses durable composite
history indexes; the query primitive alone does not attach history CLI reads.
The HTTP host now exposes these global pages at `/api/v1/fact-history`
([ADR-0237](adr/0237-owned-fact-history-http-pages.md)), with 1–100 item pages,
explicit generation-bound continuation fields, and 4 MiB bounded JSON output.
Live TCP pages match retained/current Engine results. Identity-filtered history
CLI attachment remains open; this global feed must not be mislabeled as that.
Full CI for the query/HTTP history-page addition passes in 213.04 seconds.

Unattached embedded Turso reads now retain the existing host writer lease before
store opening through query/output ([ADR-0234](adr/0234-leased-unattached-turso-reads.md)).
A process fixture covers all 19 previously unguarded read commands against an
owned invalid database, requiring rejection before opening, empty stdout, and
unchanged database bytes. This is ownership safety, not additional attachment.
Full CI for this change passes in 168.52 seconds.
The shared CLI Turso opener now takes a held lease and opens its canonical target,
not an independent pathname ([ADR-0235](adr/0235-lease-bound-cli-store-opening.md)).
This makes ownership explicit at the opening boundary; callers still retain the
guard through query/output. Extension import reuses its existing guard.
Full CI for this boundary change passes in 183.76 seconds.

Current daemon attachment covers `search-turso`, `node-turso`, `node-at-turso`,
`neighbors-turso`, `neighbors-at-turso`, `resolution-diagnostics-turso`,
`workflow-rejections-turso`, `status-turso`, and `integrity-turso`. These commands
reuse the owner's loopback HTTP boundary rather than reopening its database.
Active-owner discovery or transport failures fail closed; absent-owner operation
retains the real writer lease through embedded reads. Other read commands and
mutation dispatch are not attached. Standalone watch ownership does not itself
publish a daemon endpoint. Phase 6 remains incomplete.

`integrity-turso` now attaches to the owned backend through an Engine capability
delegate and HTTP boundary ([ADR-0233](adr/0233-owned-backend-integrity-read.md)).
Embedded/attached output and exit behavior are equivalent, including leased
fallback. Backend integrity is distinct from logical graph status and is not
cached or a cheap health probe. Full CI for this addition passes in 207.48 seconds.

CLI `status-turso` now attaches to the existing Engine for logical integrity,
workflow counts, and optional local-source freshness
([ADR-0230](adr/0230-cli-status-daemon-attachment.md)). Embedded/attached formatting
and freshness comparison are shared; tests cover current/stale refresh, foreign
scope, invalid inventories, and leased fallback. This remains a full graph scan
with a bounded response, not constant-time status. CLI inventories now use
generation-pinned retained file pages
([ADR-0232](adr/0232-paged-status-inventory-attachment.md)); total CLI memory/time,
and single-fact size remain limitations. A real publication injected between CLI
inventory pages preserves the original generation's status output. Full CI for
the retained file paging work passes in 236.37 seconds.

Read-only workflow rejection HTTP now reuses the existing Penelope journal and
shared Engine admission ([ADR-0227](adr/0227-read-only-workflow-rejection-http.md)).
TCP evidence covers typed stale-base rejections, ordered exclusive cursors,
empty results, invalid input, and unchanged journal contents/counts. The response
generation labels the observation; it does not pin a workflow-journal snapshot.
CLI rejection attachment now preserves the existing output, limit cap, and
exclusive run cursor ([ADR-0228](adr/0228-cli-workflow-rejection-daemon-attachment.md)).
The live-owner fixture verifies nonempty typed output against embedded reads and
post-shutdown leased fallback. Broader operator attachment remains open.

The store port now exposes bounded historical node pages with exclusive stable-ID
seeks ([ADR-0225](adr/0225-bounded-historical-node-pages.md)). SQLite and Turso
reuse the retained fact-tree range reader rather than replaying deltas or loading
whole snapshots. This is a paging primitive, not a diagnostic kind index.
Diagnostic HTTP/CLI attachment now reuses it with bounded node scans,
generation/repository pinning, and empty-page progress
([ADR-0226](adr/0226-bounded-diagnostic-query-and-attachment.md)). The live-owner
fixture verifies byte-identical nonempty exports and refresh after removing an
unresolved TypeScript import. A dedicated diagnostic index remains open.

`neighbors-turso` reads all outgoing HTTP pages pinned to the first generation,
validates continuations/incidence, and preserves complete CLI output and empty
missing-seed behavior ([ADR-0222](adr/0222-cli-current-neighbor-daemon-attachment.md)).
`neighbors-at-turso` now attaches with explicit generation/direction, typed edge
continuations, and 1–1000 item pages assembled through the existing HTTP route
([ADR-0224](adr/0224-cli-historical-neighbor-daemon-attachment.md)).
Attached reads retain one HTTP client throughout the command, allowing connection
reuse across neighbor pages while keeping request identity and response bounds.

Historical page export formatting is now shared through
`HistoricalNeighborPage::into_records`
([ADR-0223](adr/0223-shared-historical-neighbor-export-formatting.md)). Embedded
queries delegate to the same formatter intended for validated attached pages;
historical attachment reuses it without changing the export cursor contract.

`node-turso` and `node-at-turso` reuse guarded attachment and the common CLI node
formatter ([ADR-0221](adr/0221-cli-node-daemon-attachment.md)). Attached responses
must match the requested node and explicit generation; current and retained
historical output remains byte-identical to embedded reads. Other read/mutation
commands remain open attachment work.

Neighbor attachment preserves the existing command contract: current CLI
neighbors returns all outgoing edges and skips absent target nodes, whereas the
HTTP endpoint returns bounded historical pages and rejects a missing seed.
An adapter must pin the first returned generation for all subsequent pages,
validate endpoint/direction/ordered continuation, preserve complete output and
embedded missing-seed behavior, and reject inconsistent pages rather than
silently truncating at the HTTP page limit. Historical neighbor export must keep
its existing versioned records and cursor semantics. These mappings are
implemented for current and historical neighbor attachment; other CLI attachment
gates remain open.

`search-turso` now discovers and queries the daemon with per-request owner-instance
binding, preserving existing CLI output ([ADR-0220](adr/0220-cli-search-daemon-attachment.md)).
No-owner fallback retains a real lease across embedded store use; active-owner
discovery/transport errors do not reopen Turso. The current attachment scope is
listed above; remaining read commands and mutation dispatch are still open.

Host-only `is_writer_active` observes an existing store's cooperative writer lock
without creating files or opening the database
([ADR-0215](adr/0215-noncreating-writer-ownership-probe.md)). It reuses lease
identity and is not itself endpoint discovery or authority for direct fallback.

Held leases can atomically publish bounded discovery metadata without replacing
the lock file ([ADR-0216](adr/0216-lease-bound-atomic-discovery-publication.md)).
This reuses Shardline's write/sync/rename/directory-sync sequence and `tempfile`
cleanup. Selected CLI reads now attach as listed above; a retained record alone
is never ownership evidence.

Host-only `OwnerEndpoint` validates schema, canonical store, literal bound
loopback address, and fresh OS-random instance identity
([ADR-0218](adr/0218-validated-owner-endpoint-record.md)). It publishes through
the matching lease and decodes bounded active-owner metadata. This record does
not authenticate a listener. The daemon now publishes it after reconciliation
and the HTTP boundary checks optional owner-instance request guards before
dispatch ([ADR-0219](adr/0219-daemon-instance-bound-http-requests.md)).
Selected CLI reads now use this guard; broader attachment remains open.

`read_active_discovery` now bounds reads and requires observed ownership before
and after reading ([ADR-0217](adr/0217-bounded-active-owner-discovery-read.md)).
Inactive stale records are ignored; missing or invalid active-owner metadata is
an error. This is not atomic owner identity: instance validation and real lease
acquisition for any direct fallback remain required.

Daemon attachment must preserve the following invariants together, rather than
letting the ownership probe alone select embedded operation. The selected read
paths implement the first four; the complete process-lifecycle matrix remains
an open release gate:

- Publish endpoint metadata only while retaining the canonical writer lease and
  after initial reconciliation/readiness. Reuse Shardline's create-new temporary
  file, file sync, atomic rename, and parent-directory sync approach; do not write
  a discovery record in place or use PID existence as ownership authority.
- Bind a discovered endpoint to a specific owner instance and canonical store;
  reject stale records, replacement listeners, foreign scope, malformed/oversized
  metadata, and non-loopback destinations before sending a graph request.
- An active owner with unavailable/invalid discovery is an explicit error, not
  permission to reopen Turso. A direct fallback must acquire and retain the real
  lease through store use, handling a writer that starts between observation and
  acquisition. Endpoint identity must remain bound through each attached request.
- Reuse existing Rust HTTP/MCP handlers and query DTOs, preserving CLI output and
  retained-generation selection. Do not introduce NDJSON service handoffs or a
  second database connection into the active daemon.
- Process evidence must cover startup/readiness races, abrupt death, stale
  records, restart/port reuse, competing clients, aliases, and unchanged store
  bytes on rejected attachment. Record the concrete discovery/session contract
  in an ADR before implementation. Existing fixtures provide partial evidence;
  they do not establish this complete lifecycle matrix.

The daemon optionally serves Streamable HTTP MCP at `/mcp` on its existing
loopback listener and shared Engine ([ADR-0214](adr/0214-daemon-streamable-http-mcp.md)).
Explicit `--mcp` requires a context tokenizer. SDK session management, body limits,
and cancellation are reused under the HTTP boundary middleware. Selected CLI
reads use lease-bound discovery and local HTTP; broader session and socket
lifecycle coverage remains open.

MCP can read a trusted owner's existing Send-capable Engine without reopening
Turso ([ADR-0213](adr/0213-shared-engine-live-mcp-host.md)). Shared defaults and
response labels refresh under the Engine lock; standalone startup pinning and
explicit historical context remain intact. Selected CLI attachment is implemented
as listed above; remaining command attachment is open.

Daemon process-death and watch-failure fixtures now verify ownership release,
committed current/history retention, offline edit reconciliation, repaired-source
no-op behavior, and full StateChronicle history
([ADR-0212](adr/0212-daemon-death-and-watch-failure-evidence.md)). This does not
establish injected mid-publication crash atomicity or socket lifecycle safety.

An initial `syntaxmeshd` executable composes shared ownership, configured source
reconciliation, native/polling watch, and live HTTP
([ADR-0211](adr/0211-initial-shared-owner-daemon-host.md)). It requires explicit
Turso migration and an outside-root database; startup project policy is wired.
This is not complete Phase 6: remaining read attachment, mutation dispatch,
git/analytics/AI scheduling, configuration reload, mid-publication crash injection,
and complete socket/session lifecycle coverage remain open. Selected read
attachment currently uses the local loopback HTTP transport.

Source hosts can compose project policy into an owned mixed-language Engine and
matching planning fingerprints ([ADR-0210](adr/0210-configured-source-engine-host-setup.md)).
The watch/live HTTP fixture uses this setup. Migration, leases, semantic provider
execution, configuration reload, and daemon lifecycle remain caller concerns.

Shared hosts can reconcile structural sources through the already-owned Engine
([ADR-0209](adr/0209-shared-structural-source-reconciliation.md)). The live
watch/HTTP fixture reuses this operation; recovery precedes no-op planning.
The structural daemon now reuses this reconciliation path. Semantic provider
execution, configuration reload, and remaining daemon orchestration are open.

Local path-derived repository/worktree scope is shared with source hosts
([ADR-0208](adr/0208-shared-local-repository-scope.md)). CLI identity bytes are
unchanged; callers canonicalize roots before deriving scope. The structural daemon
uses this scope; portable Git identity remains open.

Project resolver construction and matching fingerprint suffixes are shared in
the optional source host crate ([ADR-0207](adr/0207-shared-project-resolver-setup.md)).
CLI delegates Node/Python/composite setup, preserving previous provider identity
and export-binding bytes. The existing Send+Sync provider contract supports
shared Engine hosting. Structural daemon resolver setup is wired; semantic
provider orchestration remains open.

`syntaxmesh.toml` project policy loading and non-overwriting initialization are
shared by `syntaxmesh-source-host` and reused by CLI
([ADR-0206](adr/0206-shared-project-host-policy.md)). Defaults, forward-compatible
unknown keys, known-value validation, StateChronicle opt-in, Node/Python profiles,
and ordered Python roots are preserved. The daemon loads structural resolver
policy at startup; reload and semantic provider wiring remain open.

CLI and the mixed-language shared HTTP fixture reuse the optional
`syntaxmesh-source-host` pack ([ADR-0205](adr/0205-reusable-supported-source-pack.md)).
The exact previous extension mappings and cache identity are preserved. Engine
and SDK consumers do not acquire bundled language-pack dependencies; custom
embedded registries remain supported. The structural daemon uses this pack;
remaining attachment and configuration reload remain open.

CLI and the watch/live HTTP fixture share ordered inventory fingerprint packing
and durable source planning ([ADR-0204](adr/0204-shared-source-fingerprint-packing.md)).
The fixture no longer uses an in-memory content comparison: repeated native or
polling reconciliation preserves the completed generation through the shared
planner. The structural daemon now reuses this planner and startup configuration;
remaining command attachment and configuration reload remain open.

Source no-op and transition planning now live in the runtime-neutral Engine
package and are reused by the CLI ([ADR-0203](adr/0203-reusable-source-index-planning.md)).
Existing durable marker bytes and generation identities remain compatible.
Engine callers can plan against the owned store without reopening it; scanning
and complete source/resolver configuration fingerprints remain host inputs.
The structural daemon now composes this planning with shared-owner reconciliation;
the controlled watch/HTTP fixture also reuses the shared reconciliation helper.

A real loopback integration fixture now composes the reusable native/polling
watch loop with shared-Engine live HTTP. A save becomes a verified publication
and refreshed current search while old search remains historical; shutdown
joins the watch worker before the retained Engine is dropped. This tests the
host components together. Separate daemon and CLI attachment fixtures provide
the executable/selected-command evidence; this watch fixture alone does not.

CLI watch delegates notification, debounce, reconciliation, and stop handling
to the reusable `syntaxmesh-watch-host::run_watch` callback loop
([ADR-0202](adr/0202-reusable-callback-watch-loop.md)). The CLI keeps its existing
indexing/provider logic and lease; future shared-Engine hosts can reuse the same
loop without duplicating index semantics. The structural daemon now does so;
remaining command attachment and broader lifecycle coverage remain open.

Shared HTTP hosting now accepts any Send-capable language extractor, with the
existing Rust default preserved ([ADR-0201](adr/0201-generic-shared-http-extractors.md)).
The live TCP fixture uses the SDK Send composite for Rust, TypeScript,
JavaScript, Python, Bash, Markdown, and text; it compares current/retained
symbol and heading searches after publication. The structural daemon now uses
the shared host and watch loop; Phase 6 remains incomplete as listed above.

The language SDK offers `SendCompositeExtractor` alongside its unchanged local
composition contract ([ADR-0200](adr/0200-send-capable-extractor-composition.md)).
Both share one registry implementation and identical producer/cache identity;
the Send alias requires transferable extractors without imposing Send on the
SDK trait. Generic mixed-language shared HTTP hosting remains the next wiring
step, not a capability already provided by this registry change.

Trusted Rust hosts can serve live HTTP reads through a shared Turso Engine lock
([ADR-0199](adr/0199-shared-engine-live-http-host.md)). Default query selection and
readiness use the current generation under that lock; historical selection and
standalone startup-pinned behavior remain unchanged. A TCP fixture publishes
edited source through the same Engine and verifies updated and retained reads.
Daemon watch integration, IPC, CLI attachment, and generic extractor hosting
remain open; reads and publication serialize rather than running concurrently.

HTTP search/node/generation/neighbor response labels and neighbor cursor checks
now use the admitted Engine query generation, while neighborhood serialization
uses that same query directly ([ADR-0198](adr/0198-query-scoped-http-response-labels.md)).
This preserves current startup pinning and establishes the request snapshot
boundary required for future live refresh; refresh itself remains open.

HTTP search now accepts an explicit generation and delegates to the existing
historical search query ([ADR-0197](adr/0197-generation-selected-http-search.md)).
TCP fixtures compare old/current results and labels with embedded Engine reads.
Omitted generations remain startup-pinned; live host refresh is not implemented.

Turso now resolves canonical database paths in open, migration, and migration
status, so all hosts share the real database/WAL pathname rather than only
index hosts ([ADR-0196](adr/0196-canonical-turso-backend-paths.md)). Adapter fixtures
cover alias migration/reopen/status with retained durable records and rejection
of dangling migration aliases. This does not add automatic writer ownership.

Cross-process embedded-lease fixtures exposed and corrected divergent store
alias opening: index hosts now canonicalize the backend path as well as the
lease ([ADR-0195](adr/0195-canonical-store-path-for-index-hosts.md)). File and Turso
fixtures verify parent/file alias exclusion and successful indexing after
release. This does not establish general read-host alias behavior or daemon IPC.

Watch deadline selection is shared in the runtime-neutral `syntaxmesh-watch`
crate with a caller-controlled wait cap ([ADR-0194](adr/0194-shared-watch-deadline-selection.md)).
CLI keeps its existing 50 ms signal-check cap; future hosts can reuse timing
without inheriting CLI signal or runtime dependencies.

The Shardline-derived CLI writer lease is now shared by a dependency-free
`syntaxmesh-ownership-host` crate ([ADR-0193](adr/0193-reusable-host-writer-lease.md)).
CLI writers retain the same guard lifetime and persistent sidecar behavior.
This enables daemon/embedded reuse without introducing host dependencies into
core or public contracts; other mutation APIs are not automatically coordinated.

Shared semantic extraction policy v4 now adapts Graphify's separation of named
concepts and rationale to the existing evidence-backed triple contract
([ADR-0172](adr/0172-reusable-semantic-concepts-and-complete-evidence.md)).
The controlled live comparison produced a reusable Penelope concept and fuller
rationale quotes, but relation wording and outcome-versus-motivation inference
still require review. No deterministic aliases or entailment guarantees are added.
The policy's version/hash separates old cache entries without rewriting history.

The opt-in semantic evaluator now shares its retained initial/cache/edit/repeat
workload across HTTP and local commands ([ADR-0171](adr/0171-local-command-semantic-evaluation.md)).
`SYNTAXMESH_SEMANTIC_EVAL_COMMAND` accepts the same argv-file/JSON contract; model
selection remains caller-controlled with GPT-6 Luna as the command default.
Report v5 records transport and explicitly unavailable command token usage, not
credentials or raw command arguments. Rust fixtures exercise both document-only
and cross-document command evaluation without inference in CI. This is evaluator
coverage, not a representative model-quality result.
The first live evaluator command completed the two-document initial/cache/edit/
repeat workload with GPT-6 Luna/medium and passed all cache/history checks.
Retained review exposes concept and relation wording drift after re-extraction;
entity-alias and relation-consistency work remains open rather than claimed solved.

The explicit local-command semantic transport reuses the sibling orchestrator's
argv/stdio harness pattern without its JSONL event/session protocol
([ADR-0169](adr/0169-local-command-semantic-harness.md)). Existing batching,
Penelope caching, exact evidence validation, and atomic historical publication
remain authoritative. The supplied argv fixture configures Codex / GPT-6 Luna /
medium by default, with `--semantic <model>` overriding the fixture's `{model}`
argv placeholder ([ADR-0170](adr/0170-selectable-command-semantic-model.md));
provider auth and inference stay in the external harness. Other trusted
commands can implement the same stdin/final-JSON interface. File/verified-Turso
fixtures cover packed requests, cache reuse, sparse edits, malformed/failed/
oversized responses, and historical isolation. This is not an embedded Codex API
and does not introduce NDJSON communication or source-language runtime execution.
The first small live Codex run accepted four source-supported claims from two
documents in one invocation; an unchanged repeat launched none. It establishes
account/harness operability on that fixture. A one-document edit initially failed
claim validation; retry processed only that document and reused the other cache.
The offline repeat launched none and the original historical graph hash remained
unchanged. These results do not establish representative quality or speed
superiority. Remote model revision discovery and command token telemetry remain
unimplemented; caller revisions and unavailable usage are explicit.


Local documentation-link paths now reuse Shardline's strict UTF-8 percent
decoder before the existing boundary checks and indexed lookup
([ADR-0167](adr/0167-percent-decoded-document-link-paths.md)).
Spaces and Unicode filenames work without AI or a second resolver. Extractor
identity 8 invalidates old extraction outputs; heading/history scenarios cover
both raw and encoded paths across File and verified Turso.

Upstream Graphify research confirms that semantic work is directory-local,
bounded batching with caching and adaptive splitting, not simply one invocation
per document. SyntaxMesh retains those existing mechanisms and now serializes
standard loopback Ollama inference to reduce GPU pressure
([ADR-0151](adr/0151-conservative-local-ollama-inference-scheduling.md)).
Other endpoints retain four workers. Cold/warm latency and real-model extraction
quality still need measurement; no comparative speed claim is established.

The existing one-command semantic index now automatically bisects explicitly
truncated multi-document prompts within a three-level recovery budget
([ADR-0142](adr/0142-bounded-semantic-truncation-recovery.md)). File and verified
Turso fixtures cover recovery followed by provider-free cache reuse with no new
generation. This borrows Graphify's bounded adaptive recovery without importing
its Python runtime. It does not close real-model quality or cross-document
synthesis gates; ordinary `--semantic` claims still cite one independently cached
document request. The additional `--semantic-cross-document` path is described below.
Successful recovery leaves now reuse the existing Penelope completion cache
even when a sibling fails, without partial semantic publication
([ADR-0143](adr/0143-cache-successful-semantic-recovery-leaves.md)). Restart
fixtures verify that only the failed document is dispatched on retry.

The semantic-quality path retains inexpensive extraction caching and now adds
an optional bounded joint-source layer over related original document chunks.
Synthesis over extracted claims remains a separate, unimplemented quality gate.
Reuse the existing `semantic_requests_for_generation` engine path: a controlled
joint-claim fixture now verifies two-document evidence, rejected fabricated
quotes, prepared-job retry, Penelope cache reuse, and historical isolation. No
new core request type is justified by that slice. CLI two-layer orchestration is
now opt-in through `--semantic-cross-document`
([ADR-0146](adr/0146-one-command-bounded-cross-document-enrichment.md)). It uses
the original source chunks for bounded compound requests, not generated claims.
File and verified Turso fixtures cover both layers, exact support quotations,
independent cache reuse, document edits, failed compound work without partial
publication, strict offline repeat, and historical isolation.
Compound partitioning now borrows Graphify's directory locality, ordering by
normalized parent/path and source span without exposing paths to inference
([ADR-0148](adr/0148-directory-local-semantic-batch-partitioning.md)). This is a
bounded locality heuristic, not semantic topic selection or measured quality.
The additive output-returning engine/Penelope API now makes that composition
possible without reverse-decoding graph facts or changing durable cache records
([ADR-0145](adr/0145-validated-semantic-output-composition.md)). The controlled
fixture merges two document claims and a joint claim into three claims retaining
four source-support edges. This is the reusable application boundary used by the
opt-in CLI path, not real-model quality evidence or corpus-wide synthesis.
Its cache must include all supporting document-content identities plus provider,
model, prompt, and configuration identities, so an edit invalidates dependent
synthesis rather than every document. Exact evidence must remain attached to
each supporting source; entity aliases and inferred links must not become
deterministic source facts. Corpus-wide synthesis and entity-alias resolution
remain proposed gates, not implemented capabilities, and require an ADR before
public-contract changes.
First collect real-model precision, coverage, token use, and cold/warm latency
with the existing semantic-provider evaluator; do not claim superiority to
Graphify based on mock plumbing or matching batch sizes alone.

Live semantic-quality evidence remains open. [ADR-0138](adr/0138-live-semantic-evaluation-command.md)
adds the opt-in `cargo make evaluate-semantic-provider` command with retained
accepted claims and unchanged-document cache checks. [ADR-0139](adr/0139-semantic-evaluation-source-evidence.md)
adds structured triples and hash-verified source quotations to the retained
report; positive mocks cover concept coalescing across documents and reject
changed source bytes. Mock harness validation is
not evidence of real-model extraction quality or cross-document synthesis.
The same evaluator can now opt into the two-layer workload with
`SYNTAXMESH_SEMANTIC_EVAL_CROSS_DOCUMENT=true`; report v3 retains the selected
mode, cold/warm end-to-end timing, exact source review, and joint-source claim
counts ([ADR-0149](adr/0149-cross-document-semantic-evaluation-mode.md)). Counts
are structural checks, not model precision or entailment scores.
The evaluator now reuses Shardline's initial/sparse-update workload separation:
report v4 retains a one-document edit and edited repeat, original source bytes,
edited evidence, and selective-cache/history checks
([ADR-0152](adr/0152-semantic-evaluator-sparse-edit-workload.md)).
This provides iteration-workload evidence, not real-model quality scores.
The ordinary context compiler now has end-to-end joint-document regression
coverage: seeded and lexical retrieval return both original source documents,
reuse each source read, respect serialized-budget accounting, reject stale
source bytes, and work after File-store restart. This reuses the existing
generation-pinned query path; it introduces no embedding service or new
retrieval contract and does not establish natural-language retrieval quality.
The ordinary context compiler remains current-only. Explicit
`Query::historical_context` now shares its selection and exact-token packing,
but routes node/search/adjacency/provenance reads through temporal ports and
fetches only the file versions referenced by selected nodes
([ADR-0157](adr/0157-shared-temporal-context-compilation.md)). Differential fixtures
cover seeded/lexical selection, file/provenance edits, stale-source rejection,
serialized budgets, empty requests, unknown generations/seeds, and restart.
Temporal context expansion now consumes the existing edge-only incidence pages
instead of hydrating every neighbor and discarding those payloads before
candidate selection ([ADR-0158](adr/0158-edge-only-temporal-context-expansion.md)).
A 1,001-edge port fixture verifies pagination, ordering, and no neighbor reads
on that path; the public neighbor query still hydrates its returned neighbors.
This does not bound high-degree edge scanning or establish a latency claim.
Context expansion also applies its existing selected-endpoint edge filter before
retention ([ADR-0159](adr/0159-selected-endpoint-context-edge-retention.md)), avoiding
an accumulated edge map for endpoints permanently refused by the candidate cap.
Historical adjacency now merges incoming/outgoing edge pages incrementally,
retaining at most one page per direction rather than a degree-sized vector
([ADR-0160](adr/0160-streamed-temporal-context-adjacency.md)). Stable-ID ordering,
self-edge visits, and candidate admission remain unchanged. This bounds retained
temporary edge count, not payload bytes or scanning work. Current adjacency
vectors and parallel edges among admitted nodes remain scaling limits; source
evidence, graph paths, and token packing are unchanged.
Packing now counts omitted candidate classes once and adjusts only the trial
item's class, committing counts on successful admission
([ADR-0161](adr/0161-incremental-context-omission-accounting.md)). This reuses the
existing reversible trial rather than rescanning all candidates for every item.
Raw counts retain DTO saturation semantics; exact serialized budgeting and
tokenizer rollback remain unchanged. Serialization/tokenization still perform
work per trial, so this is not constant-time context compilation.
Turso temporal incidence reads now reuse query-local prepared tree-page and
edge-hydration statements, borrowing the adapter's existing write-side pattern
([ADR-0162](adr/0162-reuse-prepared-temporal-incidence-reads.md)). SQL point-read
counts, validity checks, and ordering are unchanged; this removes repeated
preparation, not degree-proportional hydration or tree-page reads.
A bounded hydration-batch trial was rejected after its `IN` form regressed
severely and its requested-ID join failed to improve the dense debug fixture
([ADR-0163](adr/0163-bounded-historical-edge-hydration.md)). Keep the simpler
prepared point reads; the multi-page fixture now checks their instrumented
SELECT and payload-row counts explicitly. Fewer SQL calls alone are not a
performance acceptance criterion.
The real repository-reader fixture verifies matching versus changed historical
source bytes. The MCP context tool now accepts optional explicit `generation`
selection ([ADR-0154](adr/0154-explicit-historical-mcp-context.md)), validating
repository/worktree scope while preserving startup freshness and omitted-argument
behavior. Historical file and provenance point reads reuse the durable
adapters' indexed node-validity approach ([ADR-0155](adr/0155-indexed-historical-evidence-reads.md));
Temporal substring search now reuses the selected generation's persistent
node-family range walker ([ADR-0156](adr/0156-generation-root-temporal-substring-search.md)),
without replay, a full snapshot, or a new schema. It still scans node names;
constant-time lexical matching is not claimed. MCP's substantial unit suite now
uses Shardline's focused `server/tests.rs` child layout rather than living inline
in the host implementation.
Context schema v2 now carries the selecting fact's typed evidence class on every
compiled item, using one keyed store-port call for referenced provenance in the pinned
generation ([ADR-0153](adr/0153-provenance-classified-context-items.md)). Legacy
items decode with unknown classification. Original source hash validation is
independent of whether a selecting fact is inferred; quotations do not prove
claim entailment.
The public context DTO now borrows Penelope's immutable wire-fixture pattern
([ADR-0164](adr/0164-context-wire-golden-fixtures.md)): checked-in JSON is compared
with independently constructed typed request/schema-v2 values, and a legacy
schema-v1 pack must preserve its version and unknown evidence classification.
These fixtures pin names, enum spellings, identities, nulls, warnings, and
omission fields without introducing storage formats or tokenizer assertions.
The shared v3 prompt now explicitly extracts authored decision/rationale
connections, preserves proposal/requirement status, and treats source text as
untrusted data ([ADR-0150](adr/0150-source-grounded-rationale-and-joint-claim-prompt.md)).
Prompt changes invalidate identity-bound caches rather than mislabeling old
output. Separate layer prompts still require a provenance-composition decision;
the current extension manifest has one producer version.

The Engine now applies Shardline's focused sibling-module layout: generation-pinned
semantic request assembly, Penelope enrichment, and atomic replacement live in
`engine/semantic.rs`, while the eight application/integrity unit scenarios live
in the dedicated `engine/tests.rs` child module. The facade retains general
indexing, publication, status, and query composition. This is an internal layout
change: public methods, workflow/verification boundaries, IDs, and storage
formats are unchanged; no logic is moved into `lib.rs` or `mod.rs`.

**v0 setup** means architecture lock, contracts, a compiling workspace, a deterministic reference store, and one complete Rust indexing/query path. **v0.1** is the first useful release and adds the other in-scope analyzed formats and product surfaces listed in the source roadmap. SyntaxMesh implementation and execution remain Rust-only; Rust, TypeScript/JavaScript, Python, and Bash are source formats analyzed by Rust extractors, with Markdown/plain-text documentation also indexed. Node/Python resolution profiles are static strategies, not runtime dependencies. Other source languages or executable runtimes require explicit scope revision ([ADR-0078](adr/0078-rust-only-execution-and-input-language-scope.md)).

The first vertical slice should ingest a small Rust repository, persist a graph generation, restart, and answer symbol, path, and impact queries with source evidence. A file edit should update only affected facts. The same application API should work in a standalone host and an embedded test host.

The future Change Engine is a separate project. SyntaxMesh will eventually export change analysis, requirements, and verification predicates; it will not own plan execution, repository mutation, or repair workflows. See [Change Engine §§0–3, 5](syntaxmesh-change-engine-source-of-truth.md) and [SyntaxMesh §104](syntaxmesh-todo-extensible-opensource-v5.md).

## 2. Locked boundaries

| Boundary | Decision for v0 | Invariant to test |
| --- | --- | --- |
| Domain | `syntaxmesh-core` contains typed identities, facts, relationships, provenance, and generation state; `syntaxmesh-api-model` contains versioned public DTOs. | Neither crate imports Turso, Tokio, Tree-sitter, Penelope, StateChronicle, CLI, HTTP, or MCP. |
| Application | `syntaxmesh-engine` owns ingestion and query application services. Hosts only adapt transport and lifecycle. | Embedded and daemon hosts return equivalent logical results for the same generation. |
| Persistence | `syntaxmesh-store` is a port; embedded Turso WAL is the first canonical implementation; SQLite is a conformance/reference backend. Current rows serve hot reads; accepted generations/deltas are append-only rebuild history; temporal fact versions/checkpoints serve historical reads. | Atomic current-projection + history + temporal-index publication; common point-in-time reads are independent of intervening history depth; no hosted service. |
| Graph | Typed facts are canonical in the store; a generation-tagged in-memory adjacency projection serves traversals. | Read queries never observe a half-published generation. |
| Language | A versioned Rust language SDK emits definitions, references, and provenance for Rust, TypeScript/JavaScript, Python, Bash, and documentation text; resolution is a separate Rust stage. | Unresolved references are retained explicitly; parser output is deterministic; source-language runtimes are never launched. The architecture gate audits process-launch expressions. |
| Extension | Manifest, namespace, capability, and provenance contracts are public. An extension can add facts without altering core schemas. | An out-of-tree fixture can register and ingest namespaced facts using only public interfaces. |
| Runtime | Thin probes send versioned observations through `syntaxmesh-runtime-protocol`; aggregation and trust classification occur in the full engine. | Probe crates depend on neither the database nor Penelope/StateChronicle. |
| Workflow | Penelope is required inside the full engine for durable multistep operations; hot reads and individual parse calls bypass it. | Interrupted indexing resumes or reconciles; stale generation results cannot overwrite newer state. |
| Verified history | Queryable graph history is canonical and always retained from the first implementation phase; temporal indexes are the read path, while StateChronicle-backed manifest-chain verification is opt-in. See [ADRs 0026](adr/0026-current-projection-and-first-class-history.md) and [0027](adr/0027-layered-temporal-graph-queries.md). | Turning verification on changes verification status, not IDs, graph facts, or query results; this is not a signature or portable proof. |
| Analytics | DuckDB consumes a versioned analytical feed downstream of canonical generations. | Analytics failure cannot roll back or corrupt an accepted graph generation. |

These decisions follow [SyntaxMesh §§1–4, 42.4, 66.4](syntaxmesh-todo-extensible-opensource-v5.md). The [foundation ADR](adr/0001-foundation-boundaries.md) records the dependency rule.

## 3. Minimal workspace target

Create crates only when their first contract or test is ready. The initial dependency layout is:

```text
syntaxmesh-core              pure domain types, IDs, provenance
syntaxmesh-api-model         public versioned graph/query/export DTOs
syntaxmesh-runtime-protocol  lightweight observation DTOs
syntaxmesh-extension-sdk     manifest, namespace, capabilities, fact producer API
syntaxmesh-store             storage traits and conformance fixtures
syntaxmesh-store-turso       canonical embedded store
syntaxmesh-store-sqlite      reference/conformance store
syntaxmesh-language-sdk      extractor events and versioned producer identity
syntaxmesh-lang-rust         first language pack
syntaxmesh-indexer           runtime-neutral extraction and delta orchestration
syntaxmesh-workflow          durable publication and verification ports
syntaxmesh-resolver          separate reference resolution
syntaxmesh-graph             generation-tagged adjacency projection
syntaxmesh-query             search/path/impact application queries
syntaxmesh-engine            ingestion/query orchestration and embedded API
syntaxmesh-integration-penelope      required engine workflow adapter
syntaxmesh-integration-statechronicle generation verification adapter
syntaxmesh-cli               initial host and smoke-test surface
```

The Rust-native `syntaxmesh-analytics` Arrow feed, `syntaxmesh-analytics-parquet` artifact adapter, typed `syntaxmesh-analytics-duckdb` query slice, and read-only `syntaxmesh-mcp` host are implemented. DuckDB materialization, incremental synchronization, verified rebuilds, and Penelope-managed sync/rebuild orchestration are implemented; the coordinator is opt-in through the `syntaxmesh-integration-penelope` crate's `analytics-duckdb` feature and is not a default Engine dependency. The `syntaxmesh-http` host now supplies the initial read-only loopback query boundary described below. Complete HTTP/daemon acceptance and out-of-process extension hosting remain open. The initial Python and TypeScript/JavaScript packs are present; the remaining Gate 5 pack-quality work is listed below. This is a dependency plan, not an instruction to generate empty crates.

The initial HTTP host ([ADR-0177](adr/0177-read-only-loopback-http-host.md)) reuses
Shardline's Axum listener/injected-shutdown pattern and MCP's Engine query dispatch
and generation pin. It provides current search/manifest queries and retained
historical node points using the existing indexed query API. The listener is
loopback-only, with literal Host and browser-origin restrictions, sixteen admitted
blocking jobs, and stale-generation rejection. It has no indexing/migration or
mutation route and no NDJSON handoff. Daemon watching, automatic generation refresh,
authenticated deployment, bounded drain, context/traversal HTTP surfaces, and
complete host-equivalence acceptance remain open; the initial query/liveness
routes do not close those gates.
Shared context host adapters ([ADR-0180](adr/0180-shared-context-host-adapters.md))
now reuse MCP source-path checks and bounded exact-token memoization through
`syntaxmesh-context-host`. Request counters share initialized encodings but not
memoized source text. MCP keeps its profiling wrapper and historical source
hash verification remains in the query compiler. This prepares HTTP context
retrieval without moving filesystem/tokenizer dependencies into the domain.

HTTP context retrieval ([ADR-0181](adr/0181-http-context-retrieval.md)) now uses
those adapters and the existing current/historical context compiler. Explicit
root/tokenizer configuration enables bounded POST JSON requests; successful
responses are the schema-2 ContextPack directly so exact token accounting covers
the whole body. Missing configuration returns 503, unfit minimum packs return
413, and startup-generation staleness still returns 409. The route adds no
inference, NDJSON handoff, history replay, or second retrieval implementation.
Complete daemon equivalence, refresh/watch, authentication, and bounded drain
remain open.

Shutdown admission ([ADR-0182](adr/0182-http-shutdown-admission.md)) now closes
the shared HTTP query semaphore when the injected signal resolves, before Axum
drains connections. This borrows Shardline's signal-first lifecycle sequencing,
not its optional timeout: accepted blocking database workers cannot be cancelled
by dropping HTTP futures. Every host clone refuses new query work after shutdown.
Full CI passed in 121.02 seconds; bounded process shutdown and Penelope daemon
checkpoint/reconciliation remain open.

HTTP readiness ([ADR-0183](adr/0183-http-readiness.md)) borrows Shardline's
liveness/dependency-readiness separation and redacted 503 failures. GET /readyz
reports startup generation and existing Engine workflow counts without recovery
or full graph integrity scans. Prepared work, startup staleness, closed/exhausted
admission, and diagnostic errors make it unready; terminal rejection counts are
reported but do not alone block reads. Workflow diagnostics enumerate records,
so this is not a constant-time probe. TCP diagnostics/lifecycle checks and the
prepared/rejected response-policy test pass; full CI passed in 124.73 seconds.
Actual daemon recovery and ownership remain open.

CLI indexing now holds a persistent advisory sidecar lease before opening either
File or Turso stores ([ADR-0184](adr/0184-cli-index-ownership-lease.md)), copying
Shardline's RAII file-lock pattern. Cross-process rejection/release and symlink
alias tests pass; full CI passed in 130.77 seconds. Extension publication and
SQLite/Turso migration now adopt the same lease
([ADR-0185](adr/0185-cli-publication-and-migration-ownership.md)); extension
validation remains before acquisition. This is cooperating host policy, not
complete daemon ownership or embedded-writer coordination.
Full CI for the publication/migration adoption passed in 122.89 seconds.
Ownership target validation now rejects directories and dangling store symlinks
before sidecar creation ([ADR-0186](adr/0186-cli-ownership-target-validation.md)).
Regression tests retain shared leases for new stores through canonical-parent
aliases and reject directory/symlink sidecars without modifying their targets.

Watch scheduling now has a dependency-free policy component
([ADR-0187](adr/0187-runtime-independent-watch-coalescing.md)). It coalesces
notifications into a pending inventory rescan with quiet and maximum delays,
using host-supplied elapsed time and constant memory instead of a path queue.
Five deterministic tests, strict package Clippy, and architecture checks pass.
No OS notification adapter, watch command, or daemon is wired yet; hosts must
retain notifications during indexing and retry failed work through the Engine.

The separate native watch adapter now uses crates.io Notify 8.2.0
([ADR-0188](adr/0188-bounded-filesystem-watch-adapter.md)). Its nonblocking
capacity-one callback retains inventory invalidation during consumer work without
path queues; backend errors remain sticky and explicit rescan flags bypass
exclusions. Access events are suppressed to avoid read feedback. Three tests,
including a real Linux file write, strict package Clippy, architecture checks and
dependency policy pass. CLI/Engine wiring, reconciliation, restart and daemon
lifecycle remain open; notifications alone do not guarantee complete freshness.

The CLI now exposes watch/watch-turso through the existing index implementation
([ADR-0189](adr/0189-cli-watch-engine-integration.md)). A lifetime writer lease,
registration before first scan, bounded debounce, periodic inventory reconciliation
and finish-current-pass signal shutdown are wired. Stores must be outside the
source root. A real subprocess fixture verifies File/verified-Turso edits,
removals, historical retention and lease release. Separate Turso reader processes
can conflict with backend locks; same-process concurrent serving, exhaustive
startup recovery on no-op paths, bounded shutdown and full daemon IPC remain open.
Full CI passed in 436.68 seconds; the audit retry completed after a transient
registry timeout, retaining only existing allowed warnings.

Publication recovery now precedes all CLI source-fingerprint/no-op decisions
([ADR-0190](adr/0190-recovery-before-cli-noop-indexing.md)). The existing Engine
Penelope recovery method runs on the opened store before scans or marker changes;
consuming Engine::into_store mirrors existing Indexer/Penelope ownership transfer.
File/Turso subprocess fixtures fail closed on invalid durable records even when
sources are unchanged, preserving published facts. Engine transfer, prepared
publication recovery, host-equivalence/watch, strict Clippy and architecture checks
pass. This closes the publication no-op gap above, not provider-dependent semantic
job recovery, analytics recovery or the entire daemon lifecycle.

Real Unix signal fixtures now exercise idle watch shutdown/death on File and
verified Turso: SIGTERM/SIGHUP return success, SIGKILL terminates the process,
and a subsequent index reacquires ownership while retained facts and verified
history remain intact. This reuses ctrlc's existing nix dependency only for tests,
without an external signal command. Watch tests, strict CLI Clippy and architecture
checks pass; crash-during-commit and power-loss guarantees remain unproven.
Full CI for the recovery gate and real-signal fixtures passed in 151.41 seconds.

Explicit --poll-only watch fallback now bypasses native registration while reusing
the same periodic inventory/index path
([ADR-0191](adr/0191-explicit-polling-watch-fallback.md)). Native/polling File and
verified-Turso fixtures observe edits/removals and retain history. They distinguish
new publication from no-op output and require unchanged reconciliation to leave
generation count stable. Watch tests, strict CLI Clippy and architecture checks
pass; polling trades inventory reads and interval-dependent freshness for event
independence and does not remove daemon or Turso concurrent-serving gaps.

Watch waiting now follows debounce, reconciliation and timed-stop deadlines
instead of always sleeping/receiving for 50ms
([ADR-0192](adr/0192-watch-deadline-aware-waiting.md)). The dependency-free policy
reports remaining time; the CLI uses the earliest deadline with a 50ms ceiling
for signal checks. Deterministic scheduler tests, native/polling and signal
fixtures, strict Clippy and architecture checks pass. This does not add a runtime
assumption or a real-time/bounded-active-operation shutdown guarantee.

Documentation HTTP acceptance now reuses the existing composite Rust/Markdown
extractor and MCP historical-source fixture approach. Real TCP tests for both
tokenizers return decision rationale with source spans for current and restored
historical bytes; changed bytes produce `stale_source`, and deleted files
produce `source_unavailable`, without a `SourceEvidence` quotation. Retained
graph facts remain readable independently of filesystem availability. Full
`cargo make ci` passed in 126.72 seconds on Linux. This validates this fixture,
not semantic extraction quality or automatic archival of historical source bytes.

The HTTP TCP fixture covers embedded-query equivalence, retained node lookup,
invalid inputs, Host/browser restrictions, admission exhaustion and recovery,
blocking-worker permit retention after cancellation, stale-generation rejection,
non-loopback rejection, and injected shutdown. Full `cargo make ci` passes on the
implementation in 179.33 seconds on Linux; no hosted-platform result is inferred.

HTTP neighbor pagination ([ADR-0178](adr/0178-http-historical-neighbor-pages.md))
now adapts the existing `Query::historical_neighbors` and public typed cursor for
current and retained generations, incoming and outgoing. Shardline's bounded
URL-safe versioned cursor pattern supplies the host token; Engine still owns
query semantics. It adds no history replay, full snapshot load, storage migration,
or workflow. Continuations bind generation/endpoint/direction and page size stays
bounded to 100. This is single-hop paged navigation, not multi-hop/context HTTP
acceptance, automatic host refresh, or authenticated deployment.
Real TCP pagination fixtures match independent Engine pages/cursors across both
directions and current/retained generations, including a removed endpoint,
changed continuation page size, malformed/oversized/versioned tokens, cursor
binding rejection, and startup staleness. Full `cargo make ci` passes on this
implementation in 139.65 seconds on Linux.

HTTP multi-hop neighborhoods ([ADR-0179](adr/0179-http-bounded-historical-neighborhood.md))
reuse the existing indexed weak-neighborhood BFS through a pure caller-supplied
output sizer. The prior query/CLI API keeps its NDJSON export accounting; HTTP
counts its typed JSON body without constructing export records or using NDJSON
as an intermediary. Grouped limits and the sizing port permit future output
formats without a second traversal implementation. HTTP supplies hop/node/edge/
scan/body-byte caps and explicit truncation for current and retained generations.
It is not pageable, does not promise complete impact when truncated, and does not
close context HTTP, daemon refresh/watch, authentication, or bounded-drain gates.
The shared TCP fixture verifies current/retained one-, two-, and three-hop results
against independent export-path queries, preserves typed facts/depths/direction,
and exercises item/scan/JSON-byte truncation, invalid caps, removed/missing seeds,
missing generations, and startup staleness. Existing query/export tests remain
green. Full `cargo make ci` passes on the implementation in 143.31 seconds on Linux.

Dependency direction: protocols, SDK, and domain types have no inward dependency on hosts; store and extractors implement ports; engine composes them; adapters and hosts depend on engine. There must be no dependency cycle or host-only graph semantics. `syntaxmesh-engine` may depend on the Penelope integration through an internal workflow port, but every full-engine construction supplies that adapter. Thin probes and the domain remain independent.

The `syntaxmesh-extension-ipc` adapter supplies a bounded, versioned framed-JSON
codec for the existing SDK `FactBatch` ([ADR-0173](adr/0173-bounded-extension-ipc-frames.md)).
It owns neither a process nor a store and introduces no Engine runtime dependency.
This is not NDJSON. The codec alone supplies neither deployment-host peer
authorization nor deadlines, receipts, or producer lifecycle supervision. The
CLI import and independent-producer conformance evidence below close the local
file-import slice, not the long-running out-of-process host gate. Populated framed facts and
observations now have stream/Engine equivalence, File-store restart/history,
namespace rejection isolation, and Unix socket-pair publication fixtures; these
reuse the existing Engine ingestion method and Penelope publication workflow.
`FrameCodec::read_batch_for` additionally restricts decoded batches to a
caller-authorized namespace, producer version, and capability grant
([ADR-0174](adr/0174-host-authorized-extension-manifest.md)). The caller must
authenticate the peer independently; an incoming manifest cannot authorize
itself. The codec-only `read_batch` does not enforce a producer grant.
The CLI's `ingest-extension[-turso]` now imports exactly one bounded frame from
regular files into an initialized store and emits one publication receipt
([ADR-0175](adr/0175-cli-authorized-extension-import.md)). It reuses the current
Engine workflow; it is not a daemon, network peer-authentication implementation,
or producer supervisor. Imports accept an explicit final `--verify` to use the
existing StateChronicle publication path
([ADR-0176](adr/0176-extension-import-verification-opt-in.md)). This must extend
an already verified history head; verification failure may leave canonical
publication accepted and must not be treated as a safe blind retry.
The existing `extension-fixture` gate additionally runs an independent out-of-tree
producer in thin mode (normal dependencies exclude Engine/store/workflows and
their database/integration adapters), checks
verified imports into both CLI backends, and rejects its partial-header failure
without changing accepted graph or workflow diagnostics. Daemon peer/lifecycle
and timeout/kill supervision remain open; this file-import evidence does not
complete those host requirements.
The independent-producer additions passed full `cargo make ci` on Linux in
115.85 seconds; this does not establish HTTP/daemon acceptance or hosted-platform
results. The strengthened dependency guard additionally has a focused regression
test covering adapter and direct database/workflow dependencies.

## 4. Contracts to define before feature code

1. **Identity and generation.** Define `RepositoryId`, `WorktreeId`, `FileId`, `StableNodeId`, `StableEdgeId`, `GenerationId`, and `IndexRunId`; specify deterministic BLAKE3 inputs and version tags. File positions are locations, never primary identity. A generation has a base, producer/configuration fingerprint, and publication state.
2. **Fact and evidence.** Define typed node/edge kinds, provenance records, source spans, certainty/trust classes, extractor and resolver versions, and explicit unresolved references. Keep source fact, static resolution, inference, and runtime observation distinct. Reserve `FactId`/`ClaimId`, semantic generation, provider capability, and verification predicate DTO namespaces without implementing the full ontology.
3. **Atomic delta.** Define `GraphDelta` with expected base generation, changed/deleted file versions, typed fact upserts/removals, invalidation hints, and an idempotency key. The store applies it atomically and rejects stale bases. Readers bind to one published generation.
4. **Extension envelope.** Version manifests and external fact envelopes; require namespace ownership, declared capabilities, producer version, provenance, and failure isolation. Define a public conformance fixture before private integrations.
5. **Runtime observation.** Version observation DTOs and trust origin; set batching/deduplication and retention boundaries. Runtime evidence augments static facts and never silently rewrites them.
6. **Interchange.** Define versioned NDJSON header/node/edge/provenance/footer records and deterministic ordering. Export derives from canonical state; graph visualization consumes the export externally.
7. **Generation and temporal history.** Define a deterministic generation manifest/root from accepted canonical state and exact producer/configuration versions. Preserve every accepted delta with its manifest atomically alongside the current projection; build indexed temporal fact versions and checkpoints for reads, with replay reserved for rebuild/recovery. Distinguish fact-valid time from observed/accepted time; generation IDs are not timestamps. Penelope coordinates publication; StateChronicle optionally appends and replay-checks a hash-linked verification chain. Expose `DURABLE`, `VERIFIED`, and explicit failure/pending states. See [ADRs 0026](adr/0026-current-projection-and-first-class-history.md), [0027](adr/0027-layered-temporal-graph-queries.md), and [0036](adr/0036-indexed-change-and-consequence-lineage.md). Signed and portable proofs remain a later gate.

These are contract tasks. Concrete Rust type signatures, Turso table layout, and wire encoding are accepted only after a fixture demonstrates round-trip behavior and migration compatibility. The minimum indexed change/consequence history graph is decided by [ADR-0036](adr/0036-indexed-change-and-consequence-lineage.md); implementation is required before Gate 1 closes, while deeper propagation analysis remains a later product increment.

## 5. Execution sequence and exit gates

### Gate 0 — repository and architecture lock

- Put source documents under `docs/`, establish README, ADR format, threat model, license decision, and benchmark methodology.
- Draw the crate dependency graph and record prohibited dependencies in a machine-checkable rule or CI check.
- Write fixture examples for identities, provenance, `GraphDelta`, extension manifest, runtime observation, NDJSON, and generation manifest.
- Pin exact Penelope and StateChronicle versions/revisions after checking their public APIs. Confirm Rust toolchain/MSRV and Turso embedded capabilities with a small compile probe.
- Set CI gates: format, lint, unit/conformance tests, dependency rule, security audit, and migration tests once crates exist. `cargo make ci` now runs `cargo audit` and `cargo deny` in addition to the architecture and migration checks. The current policy documents two unmaintained-package notices: bincode v1, pending a compatible durable-format migration decision, and paste, required transitively by the official Parquet crate. Both exceptions remain release-review items in `deny.toml`.

Exit: every boundary above has a versioned contract and an owner; open choices are captured as ADRs; the dependency DAG compiles without placeholder implementations.

### Gate 1 — canonical persistence and generation history (in progress)

- SQLite fresh-schema SQL overlays are declared on the ordered migration registry and verified by the Rust `syntaxmesh-xtask` (`cargo make sqlite-migrations`) to be the same checked/checksummed SQL as their upgrade transition; bootstrap no longer hard-codes migration versions.

- FileGraphStore snapshot publication now stages each candidate in a unique,
  exclusively created sibling file, synchronizes its contents, then renames it
  into place; failed publication keeps the current in-memory generation and
  cleans its staging file. This is atomic visibility, not a crash-durability or
  concurrent-multiwriter guarantee ([ADR-0039](adr/0039-atomic-file-snapshot-publication.md)).
- Implement `GraphStore` in Turso WAL with an explicit schema version. Indexed Turso rows are authoritative for the current projection: open validates row projections and streams the canonical root without materializing the complete graph; it checks the latest history entry only, while the explicit backend integrity check decodes all retained history entries. Delta publication updates changed rows and the manifest in one transaction. Append-only accepted manifest/delta history is stored atomically and retained by File/InMemory. Turso/SQLite maintain indexed temporal fact versions, structurally shared content-addressed roots for every retained generation, and generation-1/every-64-generation checkpoints atomically. Historical node reads use the identity/version index; durable `GraphAt` resolves the generation root, traverses only that logical graph, and verifies the manifest root without replaying deltas or scanning prior fact versions. Checkpoints support recovery, migration, and index rebuild. The database-independent persistent-fact treap is in `syntaxmesh-store`; SQLite v4→v5 and Turso v8→v9 rebuild retained roots transactionally and each adapter updates roots in the accepted-generation transaction under [ADR-0030](adr/0030-structurally-shared-generation-roots.md). SQLite v5→v6 and Turso v9→v10 convert producer observation timestamps to lexicographically ordered big-endian values and create a partial observation-time index; SQLite v7→v8 and Turso v11→v12 add its deterministic fact-version cursor key. SQLite v6→v7 and Turso v10→v11 add per-generation acceptance-time metadata. New engine publications capture time through the runtime-neutral injected clock, persist it in the prepared Penelope operation before commit, and atomically record it with the generation; legacy operations without a captured time remain explicitly unknown. Timestamps do not affect identities or canonical graph roots. Typed `History(node)`, `FactHistory`, `GraphAt(generation)`, `ChangedBetween(a,b)`, `accepted_between`, and `observed_between` are exposed through the query service and CLI. Durable point and bounded timeline queries use temporal indexes; InMemory/File retain history scans as reference implementations. Cross-backend tests cover history depth, checkpoint boundaries, temporal-history intervals/ranges, manifest-root consistency, checkpoint-independent reads, root migration, cursor continuation, and acceptance-time recovery/restart. The three-axis validity/observation/acceptance contract and explicit historical interpretation modes are decided in [ADR-0031](adr/0031-distinct-temporal-axes-and-history-modes.md); explicit output mode labels are now required by [ADR-0035](adr/0035-explicit-temporal-query-mode.md). Exact-generation acceptance-cutoff selection is implemented by [ADR-0047](adr/0047-generation-state-as-known-by.md); this is not generic calendar-valid-time reconstruction. Semantic event/consequence lineage is implemented by [ADRs 0036](adr/0036-indexed-change-and-consequence-lineage.md), [0041](adr/0041-explicit-change-set-membership.md), and [0042](adr/0042-explicit-consequence-edges.md). Remaining Gate 1 work/evidence: total write-amplification and representative-repository storage/benchmark measurements, product-facing ChangeSet authoring access, calendar-valid-time queries where domain validity is defined, and broad transitive propagation. Checkpoint and retained-history integrity are checked by explicit SQLite/Turso backend integrity commands against history manifests and persistent roots. Point/adjacency queries remain current-projection SQL reads; explicitly full-result APIs materialize their requested output. Startup/root verification remains O(graph size), but no graph-sized heap snapshot is retained. See [ADRs 0024](adr/0024-turso-row-authoritative-storage.md), [0026](adr/0026-current-projection-and-first-class-history.md), [0027](adr/0027-layered-temporal-graph-queries.md), [0028](adr/0028-temporal-query-contracts.md), [0029](adr/0029-direct-temporal-graph-reads.md), [0030](adr/0030-structurally-shared-generation-roots.md), [0031](adr/0031-distinct-temporal-axes-and-history-modes.md), [0032](adr/0032-indexed-acceptance-timeline-query.md), [0033](adr/0033-typed-history-for-canonical-facts.md), [0034](adr/0034-indexed-observation-timeline.md), and [0035](adr/0035-explicit-temporal-query-mode.md), and [ADR-0052](adr/0052-bounded-turso-startup-history-validation.md).

- Temporal-depth baseline: `cargo make benchmark-temporal` changes one source node and one runtime-observation node on every generation, holds `GraphAt` output at two nodes and `ChangedBetween` output at ten transitions, and measures five-sample medians at 64, 256, and 1,024 retained generations. A repeat run on 2026-09-27 with Rust 1.98.1 / x86_64 Linux / AMD Ryzen 9 7950X measured historical-node / `GraphAt` / ten-transition diff / ten-item acceptance page / ten-item observation page medians (microseconds): InMemory 64=`174/170/11/21/101`, 256=`691/689/45/110/453`, 1,024=`2,753/2,742/203/742/2,478`; SQLite 64=`14/35/35/28/61`, 256=`14/36/35/29/62`, 1,024=`14/36/35/29/63`; Turso 64=`61/218/47/47/116`, 256=`62/218/47/47/134`, 1,024=`62/216/47/48/198`. Durable history-depth queries remain approximately flat for this fixture while scan-based InMemory reads grow with retained history. Full `GraphAt` is not O(1): it must materialize its requested graph output. This demonstrates depth-independent root selection and bounded-page behavior, not a general latency guarantee. Reachable pages are fetched in one recursive query. The 2026-09-27 output-size fixture (single generation, N nodes plus N-1 call edges, five samples of five `GraphAt` queries; microseconds) measured InMemory 100=`211`, 1,000=`2,553`, 5,000=`16,350`; SQLite 100=`885`, 1,000=`11,911`, 5,000=`71,699`; Turso 100=`1,505`, 1,000=`16,042`, 5,000=`102,755`. The fixed-size ten-item ChangeEvent page fixture measured medians (microseconds) at 64/256/1,024 generations: InMemory `260/1,024/4,131`, SQLite `20/21/21`, Turso `46/48/49`. The first run exposed missing event indexes on freshly initialized schemas (fresh schema initialization bypassed the upgrade that created them); after fixing fresh initialization and adding index-presence tests, durable ChangeEvent pages remained approximately flat in this fixture. It runs five samples of 50 queries and checks identical output size; storage/write amplification is not measured. All fixtures ran with Rust 1.98.1 / x86_64 Linux / AMD Ryzen 9 7950X. These synthetic runs support depth-independent durable graph-root selection and tested fixed-page reads only; they do not establish a latency SLA. Representative repositories, broad graph shapes, write amplification, page/storage growth, and hardware variability remain to be measured.
- Historical-neighbor timing follow-up (2026-09-29): `cargo make benchmark-temporal` now also times a fixed 10-edge page from a 64-edge endpoint. The history fixture holds 66 nodes and degree 64 while retaining 64/256/1,024 generations; five samples of 50 reads produced median microseconds (64/256/1,024): InMemory `1,657/6,042/27,094`, SQLite `170/172/174`, Turso `749/749/757`. A separate graph-size fixture holds queried endpoint degree at 64 and grows disconnected/unrelated facts from 35 to 4,935 nodes (100/1,000/5,000 total nodes); medians were InMemory `540/6,326/40,907` µs, SQLite `194/217/224` µs, and Turso `818/886/902` µs. Both fixtures validate page count, stable ordering, and continuation availability. These single-run synthetic timings support near-flat durable latency as retained history grows and modest growth as unrelated indexed facts increase, unlike the history-reconstructing reference store. They are not direct SQL/page-operation counts, do not isolate every cache effect, and do not prove an O(1) bound, general latency, or an SLA. Environment: rustc 1.98.1, Linux x86_64, AMD Ryzen 9 7950X.
- Historical-neighbor repeat (2026-09-29): a fresh `cargo make benchmark-temporal` run on rustc 1.98.1 / Linux x86_64 / AMD Ryzen 9 7950X repeated the fixed 10-edge-page fixtures (five samples of 50 reads, medians in microseconds). At 64/256/1,024 retained generations, InMemory measured `1,619/5,714/23,795`, SQLite `162/162/165`, and Turso `741/736/752`. At 100/1,000/5,000 graph nodes with endpoint degree fixed at 64, InMemory measured `520/5,967/35,695`, SQLite `186/211/213`, and Turso `796/855/885`. Each fixture verified result count, order, and continuation. This repeat strengthens the tested latency observation but remains one host and does not measure SQL statements/tree-page loads or prove asymptotic complexity; the operation-count acceptance item in ADR-0085/0086 remains open. Raw command output was not archived as a separate evidence bundle.
- Publication-scaling follow-up (2026-09-29): `cargo make benchmark-temporal` now measures one, ten, or 100 changed node facts over existing graphs of 100, 1,000, or 5,000 nodes (twice as many canonical node+edge facts), with five timed publications per cell and content verification outside the timer. On Rust 1.98.1 / Linux x86_64 / AMD Ryzen 9 7950X, median publication times in microseconds were:

  | backend | existing nodes | 1 changed fact | 10 changed facts | 100 changed facts |
  | --- | ---: | ---: | ---: | ---: |
  | InMemory | 100 | 10.159 | 120.427 | 1,285.583 |
  | InMemory | 1,000 | 22.452 | 229.120 | 1,882.724 |
  | InMemory | 5,000 | 57.588 | 399.811 | 2,959.023 |
  | SQLite | 100 | 1,217.856 | 1,848.089 | 5,936.973 |
  | SQLite | 1,000 | 1,522.518 | 2,783.825 | 9,583.287 |
  | SQLite | 5,000 | 2,290.689 | 7,894.056 | 25,156.797 |
  | Turso | 100 | 1,573.393 | 3,534.013 | 19,322.607 |
  | Turso | 1,000 | 1,650.007 | 4,518.892 | 24,753.720 |
  | Turso | 5,000 | 1,769.541 | 5,664.822 | 34,303.795 |

  This single synthetic run suggests durable one-fact publication grows modestly across these graph sizes, while larger changes cost more and show backend-specific scaling; InMemory also retains visible graph-size sensitivity. It does not prove O(C log F) end-to-end complexity, general latency, or a storage/write-amplification bound. The harness reports SQLite/Turso database+WAL+SHM snapshots before and after publication, but checkpoint movement can make the latter smaller (as it did for the 5,000-node fixture), so those snapshots are not bytes-written measurements. The system `/tmp` user quota was exhausted; after the task began setting Turso's temporary-file directory to an absolute path under ignored workspace `target/`, `cargo make benchmark-temporal` completed without a caller-supplied environment override. The harness also stores its DB fixtures under `target/benchmark-scratch/`. Keep representative repository and physical write-traffic evidence open.
- Store repositories, file versions, typed nodes/edges, provenance, unresolved references, producer versions, and graph generations. Preserve accepted transitions so any retained generation can be reconstructed; do not present manifest-only verification as graph history.
- Turso open-cost follow-up (2026-09-28): the temporal benchmark now also measures five-sample store-open medians at 64/256/1,024 generations. Turso measured `975/945/1,092` µs after latest-entry-only history validation; the pre-change run measured `972/1,410/3,184` µs. This suggests the history-depth component was removed for the fixed two-node fixture, while canonical-root verification remains O(current graph size); the close values at shallow depth and run-to-run variation preclude an SLA claim. SQLite measured `645/1,260/3,894` µs and still restores retained history on open as the reference backend. Old-entry payload corruption is now covered by a fixture: normal Turso open succeeds when the latest generation is valid, and explicit backend integrity reports the corrupt entry. See [ADR-0052](adr/0052-bounded-turso-startup-history-validation.md).
- Temporal-depth investigation (2026-09-28, Rust/rustc 1.98.1, Linux x86_64, AMD Ryzen 9 7950X; five-sample medians, fixed two-node graph, ten-generation diffs and ten-item pages) initially showed Turso observation pages rising from ~115 to ~196 µs across 64 to 1,024 generations in two runs. `EXPLAIN QUERY PLAN` on a 4,096-version fixture identified `SCAN syntaxmesh_fact_versions` plus a temporary sort instead of the partial observation index: Turso did not infer that the indexed column's range predicates exclude NULLs. Adding the logically redundant `observed_at_unix_nanos IS NOT NULL` predicate to both first-page and cursor-page queries makes the actual plan select `syntaxmesh_fact_versions_observed_time_idx`; a regression test checks that plan. After the fix, Turso point node / full `GraphAt` / ten-transition `ChangedBetween` / acceptance page / observation page measured in microseconds at 64/256/1,024 generations: `61.9/237.3/46.5/47.9/107.9`, `62.1/238.5/47.2/49.3/109.9`, `82.7/272.3/50.6/55.9/114.1`. SQLite observation pages remained `64.1/65.0/65.5` µs; its other point/range timings also remained approximately flat. Turso's formerly sloped bounded observation page is now nearly flat and ~42% faster at 1,024 generations in this single post-fix sample, but the remaining point/GraphAt timing variation and sample count do not establish a latency guarantee. InMemory history-scan timings continue growing strongly with depth, as expected of the reference implementation. Results validate only these synthetic dimensions/output sizes and do not establish an SLA.
- Persist versioned opaque operation records with compare-and-swap in file and Turso stores; the Penelope adapter stores its prepared delta/event log there and replays/reconciles interrupted publications.
- Ongoing schema-v2 publication no longer recomputes the public canonical root by scanning every current node and edge: it applies accepted fact mutations to the parent persistent root and computes a domain-separated semantic commitment. Existing schema-v1 manifests remain unchanged; first v1-to-v2 publication bootstraps a v2 root in O(F), and open-time/full-integrity validation may still scan all facts. Reference validation is delta-scoped on InMemory/File and durable publication paths, with full audits on restore/open and explicit integrity checks. This establishes an incremental root algorithm, not an end-to-end complexity or latency guarantee: SQL writes, changed-fact cascades, tree page writes, and retained storage growth still require changed-fact-count and representative-repository benchmarks before claiming an overall O(C log F) publication cost. See [ADR-0054](adr/0054-incremental-canonical-generation-roots.md).
- The persistent fact-tree mutation primitive now has a dev-only `proptest` oracle test: 64 generated sequences of up to 199 upserts/removes compare the current ordered contents with a `BTreeMap` and re-read sampled prior roots to check structural immutability. This protects the in-memory persistent-tree algorithm; it does not replace serialized page, durable adapter, migration, or randomized cross-backend conformance tests.
- Mixed root-version transition is now exercised against real SQLite and Turso files: each fixture seeds a schema-v1 persistent root, closes/reopens the adapter, publishes a schema-v2 child, then verifies both historical snapshots and roots after another restart. Root writers expose distinct incremental-parent and complete-snapshot-rebuild paths, so a later-generation root rebuild is explicit while ordinary incremental publication still rejects a missing parent. This verifies transition correctness, not write complexity or a performance claim.
- Live SQLite publication reuses the validated in-memory adjacency index; live Turso publication reads incident IDs from the indexed current edge projection inside the accepted-generation transaction. Both apply the same deduplicated set to persistent-root changes, temporal fact closure, and current projection deletion. The current SQLite v18 and Turso v21 schemas drop the now-write-only temporal endpoint indexes after all older migration/backfill paths have completed; those legacy paths retain their indexed endpoint-query implementations. A checksummed/ordered migration handles existing stores and fresh schemas converge. Turso retains indexed endpoints on its current edge projection; SQLite uses in-memory adjacency. The 260-node cross-backend cascade fixture checks prior/current graph reads and each edge's temporal validity interval. Three fresh-database runs against the same 696-file Shardline Rust tree measured SQLite initial indexing at 27,655 ms median versus 30,408 ms in the immediately prior baseline, one-file incremental indexing at 1,547 ms versus 1,670 ms, and retained database size at 751,677,440 bytes versus 777,555,968 bytes. Initial temporal SQL write time fell from 6,233.920 ms to 4,167.380 ms. This comparison bundles cascade-ID reuse and endpoint-index removal, so it is directional evidence for the combined change, not isolated causal attribution or an SLA; evidence is retained under `target/benchmark-results/repository-20260928T224230.156123Z-uncommitted/` and `target/benchmark-results/repository-20260928T221952.280439Z-uncommitted/`.
- A shared differential fixture now runs indexed reads, atomic rejection, file replacement/deletion, and restart checks against InMemory, File, SQLite, and Turso stores. SQLite is a distinct rusqlite adapter with bundled SQLite, transactional publication, graph-root validation, durable-record CAS, and fail-closed schema handling. Its migration runner now inspects compatibility before DDL, bootstraps fresh stores atomically from checked-in SQL, and applies a contiguous registered transition one immediate transaction at a time; rollback, unsupported-version, and unversioned-nonempty database fixtures cover failure safety ([ADR-0037](adr/0037-ordered-sqlite-schema-migrations.md)). Historical payload/index transformations remain Rust migrations; the v8→v9 schema-only event-index addition is a checked-in SQL script. Turso v5 indexes exact node names, terminal symbol names, owners, edge endpoints, and provenance references; exact/terminal resolver lookups and adjacency/file-owner reads use those indexes. Substring search still uses a SQL scan and should not be described as index-backed. The v2-to-v3, v3-to-v4, and v4-to-v5 migrations backfill columns transactionally, restart validates indexed projections against canonical payloads, and malformed legacy payloads prove failed upgrades roll back schema and version changes. Streaming open-time/root integrity verification remains O(graph size); routine schema-v2 publication updates the canonical root incrementally as described in [ADR-0054](adr/0054-incremental-canonical-generation-roots.md). End-to-end write complexity and representative changed-fact-count performance remain measurement work.
- SQLite schema is v18. Its v8→v9 adds indexed change-event tables, v9→v10 the applied-migration ledger, v10→v11 BLAKE3 checksums for SQL-backed migrations, v11→v12 moves history-anchor and event backfills out of startup, v12→v13 adds explicit ChangeSet lineage, v13→v14 adds indexed consequence history, v14→v15 adds a backfilled ancestry acceptance-prefix watermark, and v17→v18 drops redundant temporal edge-endpoint indexes after legacy backfills. `SqliteGraphStore::migrate(path)` is the explicit bootstrap/upgrade operation; `open(path)` validates the current schema, ledger, required indexes, graph, and derived projections without migration or repair, following Shardline's explicit migration/operator-startup split ([ADRs 0037](adr/0037-ordered-sqlite-schema-migrations.md), [0040](adr/0040-explicit-sqlite-migrations.md), [0042](adr/0042-explicit-consequence-edges.md), [0047](adr/0047-generation-state-as-known-by.md), and [0076](adr/0076-current-projection-edge-cascade.md)). Fresh bootstrap includes current indexes and projections atomically. The ledger is validated against the ordered registry; pre-ledger/fresh-bootstrap steps are adopted with unknown timestamps. SQL checksums are validated on open; Rust-only transforms retain null checksums. Concurrent bootstrap/upgrade attempts wait on SQLite's writer lock and revalidate committed schema advances; synchronized opener and typed pre/post-commit interruption fixtures verify rollback/reconciliation.
- The SQLite migration lifecycle now also exposes Shardline-inspired operator entry points per [ADR-0046](adr/0046-sqlite-migration-operations.md): `sqlite-migration-status` is read-only and reports applied/pending registered steps plus ledger validation; `sqlite-migrate` explicitly performs bootstrap/upgrades and reports the result. No generic `down` command is exposed because historical Rust transforms have no inverse and dropping the current consequence schema would delete authored data.
- Turso schema is v21. Its v12→v13 adds indexed `ChangeEvent` and changed-fact projections, v13→v14 adds the explicit ChangeSet journal and validity indexes, v14→v15 adds indexed consequence history, v15→v16 adds the backfilled ancestry acceptance-prefix watermark, v17→v18 installs endpoint indexes needed by the legacy temporal rebuild path, v18→v19 adds indexed node kinds, v19→v20 records module-resolution diagnostics, and v20→v21 drops the temporal endpoint indexes after that rebuild path. Fresh creation upgrades through the same registered transitions. Events are backfilled from retained generation deltas, while ChangeSet membership and consequences are deliberately backfilled empty rather than inferred. See adapter migration fixtures and [ADRs 0042](adr/0042-explicit-consequence-edges.md), [0047](adr/0047-generation-state-as-known-by.md), and [0076](adr/0076-current-projection-edge-cascade.md).
- The first indexed event projection is implemented per [ADR-0036](adr/0036-indexed-change-and-consequence-lineage.md): accepted transitions persist a `ChangeEvent` and changed-fact membership atomically, durable fact queries use the membership index, and a direct generation lookup includes no-op transitions. Durable backfill, cursor, cascade, cross-backend, and restart fixtures cover it. The v0 explicit/provenance-backed `ChangeSet` membership foundation is implemented per [ADR-0041](adr/0041-explicit-change-set-membership.md): typed core facts, atomic File/SQLite/Turso publication, additive migrations with empty legacy backfill, durable validity indexes, fixed-snapshot cursor queries, declaration point lookup, and query/NDJSON DTOs are covered by shared cross-backend, restart, and serialization fixtures. Prepared lineage and consequence publication cross the runtime-neutral workflow contract through Penelope and the Engine, with opt-in StateChronicle generation verification preserved. Consequence publication is atomic in File/SQLite/Turso and carried through the v4 Penelope digest-bound journal; v1–v3 remain replay-compatible. The store and query API now provide cursor-paged exact-endpoint reads and deterministic bounded weakly connected BFS at a pinned generation under [ADR-0043](adr/0043-indexed-consequence-endpoint-reads.md) and [ADR-0044](adr/0044-bounded-consequence-neighborhood.md). Durable adapters use temporal endpoint indexes; reference stores derive from retained history. Fixtures cover publication/restart, SQL endpoint pagination and fact-version lookup, BFS bounds, versioned NDJSON/CLI export, and injected consequence-add/retraction rollback in SQLite/Turso. [ADR-0047](adr/0047-generation-state-as-known-by.md) now qualifies an exact retained graph generation by an engine-acceptance cutoff using a transactionally maintained ancestry maximum, depth-independent durable point reads, unknown legacy propagation, graph NDJSON v4, and `graph-at-known-by[-turso]`. This is scoped modeled-generation/knowledge-time selection—not calendar-valid-time reconstruction or current-semantics retrospective evaluation. Remaining: total write-amplification and representative-repository storage accounting, product-facing ChangeSet authoring access, generic calendar-valid-time queries, and broad transitive propagation. Historical answers must use temporal indexes, not delta replay.
- Scope clarification per [ADR-0050](adr/0050-changeset-authoring-ownership.md): every Gate 1 reference to “product-facing ChangeSet authoring access” (including the earlier scope summary and later implementation-status notes) is superseded; authoring belongs to the separate Change Engine and is not a SyntaxMesh blocker. SyntaxMesh provides prepared lineage publication and read-only temporal queries; it does not add a second authoring/mutation workflow.
- Persistent-root batch writes now retain only newly allocated treap pages reachable from the committed root; copy-on-write intermediate pages superseded during bulk mutation are discarded before SQLite/Turso persistence. A focused `PersistentFactTreeCache` regression test proves this without weakening root traversal or history invariants.
- SQLite current-projection publication now mirrors the useful Shardline local-SQLite pattern: transactionally upsert changed facts and delete removed facts instead of clearing and rewriting every current row. Removed-node edge cascades are derived from the prior in-memory adjacency indexes; unchanged rows are preserved. A trigger-based regression fixture proves a one-node change neither deletes nor updates unaffected rows. Historical deltas, temporal fact versions, and root publication remain in that same SQLite transaction.
- Following Shardline's focused `local_sqlite` module layout, SyntaxMesh's SQLite path validation, open flags, security/durability pragmas, busy timeout, and WAL setup now live in `backend/connection.rs`; all 14 transactional migration steps live in `backend/migration_steps.rs`, separate from the ordered registry in `backend/migrations.rs`. The migration runner and public lifecycle remain unchanged, and existing migration, checksum, rollback, symlink, and concurrent-open fixtures continue to cover those paths. We intentionally retain `BEGIN IMMEDIATE` and the existing 30-second busy handler instead of copying Shardline's whole-transaction retry wrapper, which assumes a fresh connection per attempt; retrying a long-lived `SqliteGraphStore` connection needs separate idempotency/commit-boundary design.
- Following Shardline's `local_sqlite/tree_store.rs` boundary, SQLite persistent-fact tree mutation, page loading/writes, generation-root publication, temporal snapshot loading, and persistent-root `GraphAt` reads now live in `backend/tree_store.rs`. Transaction sequencing remains in the graph store; SQL statements, ordering, and atomic publication behavior are unchanged.
- Following Shardline's `local_sqlite/record_store.rs` boundary, SQLite's `DurableRecordStore` implementation and Unicode-safe prefix range helper now live in `backend/record_store.rs`. CAS transaction and cache-reconciliation behavior remain unchanged.
- SQLite's physical database and retained-checkpoint integrity implementation now lives in `backend/integrity.rs`, matching Turso's `adapter/integrity.rs`; explicit integrity-command behavior and checkpoint validation are unchanged.
- SQLite's complete `GraphStore` implementation is now isolated in `backend/index_store.rs`, following Shardline's `local_sqlite/index_store.rs`; all generation-pinned reads and their freshness guards remain together, while schema lifecycle and write orchestration stay in the adapter.
- Turso's opaque durable workflow-record queries and transactional compare-and-swap implementation now live in `adapter/record_store.rs`, matching SQLite's `backend/record_store.rs` and Shardline's `local_sqlite/record_store.rs`; runtime ownership and error mapping remain with the Turso adapter.
- Turso's `GraphStore` port implementation now lives in `adapter/index_store.rs`, alongside SQLite's `backend/index_store.rs` and Shardline's `local_sqlite/index_store.rs`. The implementation delegates publication to the existing transactional adapter methods; no transaction boundaries or query contracts changed.
- Turso's registered schema-transition implementations now live in `adapter/migration_steps.rs`, mirroring SQLite's `backend/migration_steps.rs`; schema probing, WAL validation, ledger verification, and status reporting stay in the adapter. The ordered migration registry and transition behavior are unchanged.
- Turso's temporal fact-version and persistent-tree operations now live in `adapter/temporal_tree.rs`, parallel to Shardline's focused tree-storage boundary. The adapter continues to own transaction sequencing and publication; this internal responsibility split changes no SQL, persistence, or public contract.
- Applying the same responsibility-oriented module boundary to Turso, `adapter/integrity.rs` now owns indexed-column consistency, canonical current-projection/root validation, full and delta reference checks, legacy root streaming, retained-history validation, and checkpoint validation. It imports its adapter dependencies explicitly; public store behavior and transaction boundaries are unchanged. This makes integrity changes local without moving database semantics into core or the public store contract.
- ADR-0054 schema-v2 roots are implemented across InMemory, SQLite, Turso, and Engine status integrity: roots commit canonical file/provenance/node/edge facts through the shared persistent-fact tree; v1 manifests stay verifiable, the first v2 publication bootstraps from the full accepted snapshot, and later durable publications apply delta mutations. Turso open-time validation compares the current projection with the persisted fact tree; historical reads dispatch root verification by manifest version. Engine integrity status also dispatches by root schema. `cargo make ci` passes, including the v2 host-equivalence path.
- Post-v2 real-workload benchmark (`cargo make benchmark-repository`, 2026-09-28, Rust 1.98.1/rustc 1.98.1, Linux x86_64, AMD Ryzen 9 7950X) scanned 48 Rust files / 1,325,671 source bytes and produced 16,572 nodes / 19,854 edges / 49 provenance facts. Initial / one-file incremental indexing measured InMemory 4,913/4,755 ms, SQLite 6,366/4,723 ms, Turso 9,445/53 ms; closed sizes were SQLite 90,800,128 bytes and Turso 102,537,624 bytes. The changed input is a 148-byte in-memory comment append. This one run shows the durable Turso delta path is fast on this fixture, but does not establish a repeatable gain; SQLite and InMemory remain slow, and database size grew. Investigate backend timing breakdown, benchmark variance, and write amplification before making performance claims.
- Penelope repository benchmark after [ADR-0060](adr/0060-resumable-persistent-tree-mutations.md) (`SYNTAXMESH_BENCH_ROOT=/home/ac/projects/penelope`, 2026-09-28, Rust/rustc 1.98.1, Linux x86_64, 32 logical CPUs) scanned 43 Rust files / 466,599 source bytes and produced 3,734 nodes / 4,610 edges. Across three fresh database runs, median initial / one-file incremental indexing was InMemory 303/167 ms, SQLite 514/474 ms, and Turso 1,717/1,212 ms; retained database sizes were 16,658,432 bytes SQLite and 19,202,152 bytes Turso. A prior single pre-change sample on the same workload recorded SQLite 512/9,849 ms and Turso 1,683/10,626 ms; the new incremental timings are a strong signal that resuming after lazy page loads avoids substantial replay, but the pre-change sample count is one, so this is not a statistically matched speedup claim. The three-run raw evidence bundle is ignored at `target/benchmark-results/repository-20260928T103227.000049Z-uncommitted/`; it includes per-stage RSS and database page accounting. This does not measure write amplification or establish an SLA.
- Fixed-output benchmark re-run with `cargo make benchmark-temporal` on 2026-09-28 (Rust 1.98.1, Linux x86_64, AMD Ryzen 9 7950X; five-sample medians; graph output held to two nodes; 10-generation diffs and 10-event pages) now bootstraps SQLite through the explicit migration API and reports closed-database bytes including any WAL/SHM sidecars. Timings are nanoseconds; bytes are persisted database size. This synthetic result validates history-depth independence for indexed point/page queries, not an SLA or total write-amplification measure:

  | Depth | Backend | Historical node | GraphAt(2 nodes) | Diff(10 generations) | ChangeEvents(10) | Persisted bytes |
  | ---: | --- | ---: | ---: | ---: | ---: | ---: |
  | 64 | SQLite | 13,389 | 35,669 | 34,271 | 20,545 | 528,384 |
  | 256 | SQLite | 13,713 | 34,609 | 34,605 | 20,541 | 1,404,928 |
  | 1,024 | SQLite | 13,949 | 35,155 | 34,791 | 21,177 | 4,722,688 |
  | 64 | Turso | 61,906 | 214,255 | 47,281 | 47,790 | 2,003,448 |
  | 256 | Turso | 63,346 | 217,719 | 46,828 | 47,037 | 2,900,568 |
  | 1,024 | Turso | 63,264 | 220,791 | 47,221 | 48,489 | 7,116,392 |

  Persisted size is now recorded, but write amplification still needs a logical-bytes-written baseline and representative repositories/graph shapes. A real-workload harness is available as `cargo make benchmark-repository` (set `SYNTAXMESH_BENCH_ROOT` to the source tree; absent an override, the harness discovers its Cargo workspace root). On SyntaxMesh itself, Rust 1.98.1 / Linux x86_64 / AMD Ryzen 9 7950X, a single run at 2026-09-28 03:37 UTC scanned 48 Rust files / 1,306,997 source bytes in 2,096 μs; Engine indexing produced 16,380 nodes and 19,626 edges. Initial / one-file incremental indexing measured InMemory 2,135/474 ms, SQLite 3,590/438 ms, Turso 9,141/187 ms. Closed database size after two generations was SQLite 68,616,192 bytes and Turso 80,226,640 bytes. The measured compiler was rustc 1.98.1 (48a229cea 2026-09-01). These are single-run local evidence, not an SLA; source bytes are not physical bytes written, so they do not establish write amplification. The measurements expose substantial remaining storage expansion to investigate. The earlier `dbstat` inspection attributed ~13.5 MB to persistent tree pages, 8.7 MB to temporal fact payloads, ~5.6 MB each to Penelope records, generation deltas, and checkpoints, ~7.2 MB to current nodes/edges, plus event-fact indexes and other lookup indexes. No payload deduplication is assumed by those numbers. Fixed-output history-depth benchmarking is implemented; Gate 1 still requires more representative repositories/graph shapes, storage/write-cost measurement, and broader calendar-valid-time semantics only where fact domains define them. Acceptance-cutoff selection for exact modeled generations and the initial indexed event/consequence lineage are implemented (ADRs 0036, 0041–0044, 0047). These measurements were taken on a warmed local filesystem and are reproducibility evidence only.
- Following Shardline's `scripts/benchmark-matrix.sh` evidence-bundle pattern, `cargo make benchmark-repository-evidence` runs through the Rust `syntaxmesh-xtask` and wraps the existing Rust fixture in fresh-database repetitions. It records raw stdout/stderr, parsed per-sample metrics, host/toolchain/revision metadata, SQLite-compatible per-table/index `dbstat` page and payload bytes, and a median summary under ignored `target/benchmark-results/`. Set `SYNTAXMESH_BENCH_ITERATIONS` to change the default of three and `SYNTAXMESH_BENCH_ROOT` to select the repository fixture. This improves reproducibility and retained-storage attribution; it does not change benchmark semantics or measure physical write amplification.
- The Linux write-syscall harness additionally records the serialized size of the latest accepted `GenerationHistoryEntry` payload (manifest plus canonical delta) for each database stage. This makes syscall bytes per history-payload byte observable without treating source bytes as the denominator; it is still only a partial ratio, not total write amplification, because mmap writeback, indexes, page overhead, and other durable records are outside that payload.
- Verified smoke run on the two-file `syntaxmesh-core` fixture (2026-09-28, rustc 1.98.1, Linux x86_64): initial latest history payload was 99,510 bytes for both SQLite and Turso; the 143-byte incremental source change produced a 928-byte history payload. Successful file-write syscall bytes were 2,653,856 / 1,557,360 (initial) and 398,528 / 218,360 (incremental), respectively, yielding partial syscall-to-history-payload ratios of 26.7 / 15.7 and 429.4 / 235.3. These are smoke measurements on a tiny fixture, not representative write-amplification claims. The measurement probe explicitly closes its read-only SQLite-compatible connection before proceeding to another Turso stage.
- Representative sibling-project smoke measurements (2026-09-28, one fresh-database sample, Rust/rustc 1.98.1, Linux x86_64, AMD Ryzen 9 7950X) used the above isolated evidence harness. Penelope (43 Rust files / 466,599 bytes; 3,734 nodes / 4,610 edges) measured initial/one-file incremental indexing at InMemory 297/170 ms, SQLite 512/9,849 ms, and Turso 1,683/10,626 ms; retained SQLite/Turso sizes were 16,658,432/19,202,152 bytes. StateChronicle (168 Rust files / 1,906,948 bytes; 16,637 nodes / 20,794 edges) measured InMemory 926/867 ms, SQLite 1,954/50,538 ms, and Turso 7,284/78,044 ms; retained sizes were 73,347,072/85,090,432 bytes. The changed input was one appended comment (7,420 bytes in Penelope; 9,144 bytes in StateChronicle). Both runs completed correctness/status checks across all three backends. The unexpectedly long durable incremental samples are a profiling target, not an SLA or a stable comparison; each sibling currently has only one sample. StateChronicle initially exposed identical-content source provenance ID collisions; [ADR-0059](adr/0059-file-scoped-extractor-provenance.md) fixes the shared source-extractor identity derivation, and the successful rerun evidence is under ignored `target/benchmark-results/repository-20260928T101836.765902Z-uncommitted/` and `target/benchmark-results/repository-20260928T102136.908808Z-uncommitted/`.
- The same evidence bundle follows Shardline's isolated-run resource accounting pattern: it launches InMemory, SQLite, and Turso benchmarks in separate processes, and GNU `/usr/bin/time` records maximum RSS for each backend process tree. The summary reports per-backend medians. This distinguishes backend peaks without changing engine or store contracts; other hosts explicitly report RSS unavailable.
- Instrumentation smoke sample on `crates/syntaxmesh-core` (2 Rust files, 36,750 source bytes, 281 nodes, 299 edges; one run, Rust 1.98.1/Linux x86_64) recorded 79.6 MiB peak process-tree RSS. Backend initial/incremental times were InMemory 27/25 ms, SQLite 39/28 ms, and Turso 111/3 ms. This tiny one-run sample validates resource capture only; it is not representative memory evidence or an SLA. Raw evidence is under ignored `target/benchmark-results/repository-20260928T073805.370918Z-uncommitted/`.
- The first Shardline-scale RSS sample (696 Rust files, 174,082 nodes, 217,693 edges) peaked at 16,380.5 MiB across the complete benchmark process tree. This exposed excessive transient persistent-tree pages during bulk root construction. The shared tree now prunes unreachable copy-on-write cache pages in bounded batches and uses a linear Cartesian-tree build for sorted upsert-only empty-root batches; a regression compares bulk roots with incremental insertion, including compact values with canonical commitments. A repeat retained identical graph counts and SQLite/Turso sizes (767,135,744 / 891,293,800 bytes), with 6,702.9 MiB peak RSS. In one-run same-host evidence, total elapsed time fell from 721.9 s before the cache change to 403.1 s after both changes; backend initial/incremental timings were InMemory 19,828/14,205 ms, SQLite 36,265/46,205 ms, and Turso 112,930/150,341 ms. This comparison is uncommitted and single-sample, not a repeatable performance claim. Residual 6.7 GiB peak is still high and backend/phase attribution remains necessary. Evidence: `target/benchmark-results/repository-20260928T073858.567334Z-uncommitted/` (before) and `target/benchmark-results/repository-20260928T081920.164337Z-uncommitted/` (after).
- After delta-scoped reference validation landed, a repeat SyntaxMesh run at 2026-09-28 04:09 UTC (Rust/rustc 1.98.1, Linux x86_64, AMD Ryzen 9 7950X) scanned 48 Rust files / 1,313,524 source bytes and produced 16,457 nodes / 19,724 edges. Initial / one-file incremental indexing was InMemory 2,153/473 ms, SQLite 3,653/441 ms, Turso 9,312/165 ms; closed sizes were SQLite 68,964,352 bytes and Turso 80,574,680 bytes. The one-file change is a 148-byte comment append that changes the file hash but not graph facts. Against the 03:37 sample, incremental timings are effectively unchanged for InMemory/SQLite and modestly lower for Turso; a single warmed run cannot establish a speedup. ADR-0054 now introduces schema-v2 canonical roots backed by the persistent fact tree: regular v2 publication applies the accepted delta to the parent root, with one O(F) v1-to-v2 bootstrap. Open-time validation and explicit integrity scans remain O(F); no speedup claim is made until a post-change representative benchmark is recorded.
- Post-ADR-0054 representative repeat (2026-09-28; 3 fresh database runs, uncommitted SyntaxMesh workspace; rustc 1.98.1, Linux x86_64, AMD Ryzen 9 7950X) scanned the same Shardline fixture (696 Rust files, 14,669,297 source bytes) and produced 174,082 nodes / 217,693 edges. Median initial / one-file incremental indexing was InMemory 61,305/81,252 ms, SQLite 92,114/116,006 ms, and Turso 128,020/154,183 ms. Median final database sizes were SQLite 978,972,672 bytes and Turso 1,103,134,824 bytes. Per-sample timings and raw logs are preserved in the ignored `target/benchmark-results/repository-20260928T061226.050387Z-uncommitted/` bundle. The timing samples were closely grouped, but this is still one host and an uncommitted worktree, not a cross-version regression claim or SLA. Its `dbstat` totals prompted [ADR-0055](adr/0055-compact-schema-v2-persistent-fact-values.md) to separate canonical fact commitments from compact physical page values.
- One-run verification after ADR-0055 (same Shardline source tree and machine; one fresh SQLite/Turso database) retained 174,082 nodes / 217,693 edges and passed graph-root/reference checks. The `syntaxmesh_temporal_tree_pages` table allocation fell from 389,844,992 to 177,094,656 bytes (payload 341,641,963 to 164,131,496). Total final size fell from the previous three-run medians 978,972,672 to 767,135,744 bytes for SQLite and 1,103,134,824 to 891,293,800 for Turso. In this run, initial/incremental timings were InMemory 66,408/81,195 ms, SQLite 89,103/110,780 ms, and Turso 127,318/150,403 ms. That is direct storage-attribution evidence; the post-change timing has only one sample and is not a speedup/SLA claim. The complete per-table/index page and payload listing is in `target/benchmark-results/repository-20260928T071554.176997Z-uncommitted/` (ignored local evidence).
- Isolated per-backend RSS and phase instrumentation on the same Shardline tree (2026-09-28, one fresh process/database per backend) retained 174,082 nodes / 217,693 edges. GNU time process-tree peak RSS was 5,794,360 KiB InMemory, 6,828,736 KiB SQLite, and 2,353,404 KiB Turso. `/proc/self/status` high-water samples show the InMemory and SQLite peaks occurring during `index()`: initial-index peak growth was 3,258,876 / 3,438,336 KiB, then incremental-index added another 2,499,752 / 3,354,684 KiB; their `status()` phases added no peak. Turso reached its 2,317,576 KiB peak growth during initial indexing and added no further process high-water during the incremental pass. This local one-run evidence prioritizes investigation of per-generation indexing allocations and retained working sets; it does not identify a specific allocation source or establish a speedup/SLA. Raw stage marks and logs are in `target/benchmark-results/repository-20260928T084217.644314Z-uncommitted/` (ignored local evidence).
- Heap profiling of the 58-file SyntaxMesh workload located avoidable edge-value duplication in InMemory's derived incoming/outgoing indexes. Those caches now retain edge IDs and read values from the canonical edge map. A matched one-run Shardline comparison (696 files, 174,082 nodes, 217,693 edges) reduced GNU time process-tree peak RSS from 5,794,360 to 3,459,700 KiB for InMemory and from 6,828,736 to 3,911,140 KiB for SQLite (~40% and ~43%); Turso remained effectively unchanged (2,353,404 to 2,352,932 KiB). Initial/one-file incremental times were 19,197/13,906 ms vs 18,789/12,826 ms for InMemory and 36,521/46,376 ms vs 36,070/44,879 ms for SQLite, so the single samples do not show a consistent timing gain. This is a derived-index representation change with no public contract or serialized snapshot change. Raw post-change evidence is in `target/benchmark-results/repository-20260928T085522.269918Z-uncommitted/`; memory remains above 2 GiB and requires further profiling.
- Follow-up Massif attribution on the same large in-memory workload found substantial avoidable peak allocations in generation-root construction: JSON serialization buffers (about 402 MB of direct `Vec` reserve allocations, including about 155 MB for edge values and 128 MB for source locations), plus roughly 79 MB retained by the persistent-tree cache. `InMemoryGraphStore::root_v2` now streams its ordered canonical maps into a sorted mutation vector and consumes that vector in the bulk tree builder, avoiding the full cloned `GraphSnapshot` and a second copy of mutation payloads; snapshot-based root computation uses the same owned bulk path. A root-oracle regression test checks the result against the prior full-snapshot implementation. On the same 696-file / 174,082-node / 217,693-edge Shardline fixture, one-run process-tree peaks after this change were 3,050,596 KiB InMemory and 3,501,528 KiB SQLite, versus the immediately preceding 3,459,700 / 3,911,140 KiB samples (about 12% / 10% lower). Initial / one-file incremental timings were 18,182/12,375 ms and 35,844/44,132 ms respectively. Each point is a single local sample, not an SLA or a stable speed claim; Turso was not rerun because its root path does not use this InMemory snapshot builder. The Massif profile is in ignored `target/shardline-inmemory.massif`; benchmark results above were captured from command output and are not archived as evidence bundles.
- Second real-workload sample: `SYNTAXMESH_BENCH_ROOT=/home/ac/projects/shardline cargo make benchmark-repository` scanned 696 Rust files / 14,669,297 source bytes in 24,879 μs and indexed 174,082 nodes / 217,693 edges. On Rust 1.98.1, Linux x86_64, AMD Ryzen 9 7950X, one run measured initial/one-file incremental times of InMemory 16,408/10,469 ms, SQLite 40,819/40,094 ms, and Turso 111,111/97,887 ms. After two generations, persisted database sizes were SQLite 814,989,312 bytes and Turso 939,242,152 bytes. The 5,000-byte incremental input was an in-memory comment append to the first file. This is a single-run, warmed-local-filesystem workload measurement; persisted size is not physical bytes written and does not establish write amplification or an SLA. The benchmark's temporary database directory was removed after the run.
- `dbstat` inspection of that two-generation sample found 65,514,741 bytes of payload in two completed Penelope records (the larger request was ~65.4 MB), alongside ~159 MB of temporal-tree pages and ~103 MB of fact versions in SQLite. [ADR-0049](adr/0049-compact-completed-penelope-records.md) now keeps full deltas only while workflows are prepared and stores a digest-bound Penelope receipt/events after completion. A 256 KiB serialization regression verifies the compact completed record is under 1% of the prepared payload while preserving replay and rejecting changed requests. This reduces live record/WAL/backup payload; it does not shrink existing database files or alter high-water file size without a separately designed maintenance operation.
- The current compact receipt is v6: separate request and process-definition digests preserve exact-retry validation and replay of legacy v1-v3 Penelope event definitions. v1-v5 continue decoding; exact retries CAS-compact completed v1-v4 rows, while startup remains read-only. See [ADR-0049](adr/0049-compact-completed-penelope-records.md).
- Post-compaction repeat on the same Shardline 696-file workload produced 749,510,656 SQLite bytes and 873,664,616 Turso bytes, versus the earlier 814,989,312 and 939,242,152 bytes respectively. SQLite and Turso `dbstat` each report the Penelope record table at one 4 KiB page with 1,650 payload bytes for both completed records, versus 65,514,741 bytes before compaction. The ~65 MB persisted-size reduction per backend matches removing the two duplicated completed requests; timings varied by run and are not used to claim an indexing speedup. The repeat is still a single-run local comparison, not a write-amplification or SLA result; its temporary database directory was removed.
- Fresh Shardline workload repeat on 2026-09-28 (rustc 1.98.1, Linux x86_64, AMD Ryzen 9 7950X) scanned 696 Rust files / 14,669,297 source bytes in 20,082 μs and produced 174,082 nodes / 217,693 edges. Initial / one-file incremental indexing measured InMemory 16,567/9,202 ms, SQLite 42,531/42,113 ms, and Turso 110,069/156,980 ms. Closed sizes were SQLite 749,510,656 bytes and Turso 873,668,712 bytes, effectively matching the prior post-compaction sample. During this run, the open Turso WAL was observed at 6.0 GiB before checkpoint; this is a sampled file size, not cumulative writes or a measured maximum. Timing remains a single run and is not evidence of a regression or SLA. The fixture was removed after the benchmark.
- Three-run Shardline repository evidence bundle on 2026-09-28 (rustc 1.98.1, Linux x86_64, 32 logical CPUs) re-scanned 696 Rust files / 14,669,297 source bytes and consistently produced 174,082 nodes / 218,694 edges / 697 provenance records. Across three fresh databases, median initial / one-file incremental times were InMemory 18,171/12,026 ms, SQLite 36,428/24,084 ms, and Turso 112,090/132,037 ms. Median retained sizes were 769,024,000 bytes SQLite and 893,624,424 bytes Turso; measured median peak process-tree RSS was 2,984.6 MiB InMemory, 3,425.4 MiB SQLite, and 2,303.9 MiB Turso. Thus Turso was consistently slower and retained ~16% more bytes than SQLite on this workload, despite lower measured peak RSS; the gap is now a concrete profiling target, not a performance claim. The source tree was read-only; the 5,000-byte incremental edit existed only in memory, and fresh per-backend databases/raw logs are retained under ignored `target/benchmark-results/repository-20260928T120134.637164Z-uncommitted/`. `dbstat` page/payload values describe retained pages, not bytes written, so total write amplification remains unmeasured. The harness's aggregate initial/incremental timings do not isolate resolver, Penelope, root/tree mutation, SQL, or WAL costs; add stage attribution before attempting an optimization.
- Stage attribution added to `cargo make benchmark-repository-evidence` and checked against a smaller Penelope repository and one fresh Shardline sample. The Shardline run reproduced 174,082 nodes / 218,694 edges; InMemory, SQLite, and Turso total times were 18,112/12,081 ms, 35,673/23,711 ms, and 111,579/132,975 ms (initial/incremental). Median stage times are not yet available for this one-sample run, but the measured initial prepare-delta times were nearly identical at ~9.8 s for every backend; Turso's initial Penelope-publish bucket was 101.8 s versus SQLite 25.9 s, while incremental preparation was 217 ms Turso versus 32 ms SQLite and incremental publish was 132.8 s versus 23.7 s. On the smaller 43-file Penelope fixture, Turso also spent 1.50 s / 1.14 s in publish versus 186 ms / 49 ms in preparation. This localizes the Shardline gap to the combined Penelope/publication-and-store-commit phase, not source parsing/reference resolution; the bucket still includes workflow journal/CAS plus the adapter's atomic graph transaction, so it does not yet distinguish Penelope serialization from Turso mutation cost. The profiler uses the same runtime-neutral `Indexer` and `PenelopePublisher` composition and validates each generation's v2 root and fact references; it changes no production API or behavior. The Shardline profile evidence is retained under ignored `target/benchmark-results/repository-20260928T122609.702246Z-uncommitted/`. Next, split the publication bucket into durable-record prepare/CAS, graph transaction/root update, and completion-record CAS before changing store or workflow code. These are local single-run attribution measurements, not an SLA or write-amplification result.
- A trial of Linux `/proc/self/io` `write_bytes` was rejected as a database write-amplification measure in this environment: the same 696-file workload attributed only 2,097,152/0 bytes to SQLite initial/incremental indexing and 0/0 bytes to Turso, despite closed database sizes of ~750/~874 MB. This process-level counter is not reliably scoped to adapter database writes here. An opt-in Linux `cargo make benchmark-file-writes` now uses Shardline-style isolated benchmark fixtures and `strace -P` path attribution to count successful write-family syscalls for each database/WAL/SHM set. A two-file `syntaxmesh-core` validation run observed 3,093,328 SQLite bytes and 2,278,368 Turso bytes across bootstrap plus two generations. This validates attribution only: ptrace materially distorts timings, mmap dirty-page writeback is omitted, and bootstrap is included, so it is not total physical I/O or a complete write-amplification result. True write amplification remains open until per-generation logical bytes and every backend's transaction/journal/page writes can be compared; persisted size and `dbstat` only describe retained storage.
- The Linux write-syscall harness now follows Shardline's evidence-bundle convention: each run preserves metadata (including fixture/toolchain/host/strace/revision), per-stage/backend stdout and stderr, raw path-filtered syscall traces, structured results, and a Markdown summary under ignored `target/benchmark-results/write-syscalls-*`. A repeated two-file `syntaxmesh-core` validation run (36,965 source bytes, 281 nodes, 303 edges) recorded bootstrap/initial/incremental successful write-family syscall bytes of SQLite `274,432/2,653,856/398,528` and Turso `506,768/1,557,360/218,360`. Source bytes are retained as workload context only, not used as the write-amplification denominator; mmap and physical-device writes remain unmeasured. Raw evidence is in `target/benchmark-results/write-syscalls-20260928T104947.564513Z-uncommitted/`.
- Staged write-syscall benchmark follow-up (2026-09-28, Rust 1.98.1, Linux x86_64, AMD Ryzen 9 7950X) now traces bootstrap, initial indexing, and one-file incremental indexing in separate processes against persistent isolated SQLite/Turso fixtures. A `syntaxmesh-core` sample (2 Rust files, 36,750 total source bytes, 281 nodes, 299 edges; incremental source file 143 bytes) observed successful DB/WAL/SHM write-family syscall bytes: bootstrap SQLite/Turso 274,432/506,768; initial index 3,253,624/1,858,120; incremental index 497,120/263,680. Every stage verified graph-root and reference integrity, and the incremental stage required the exact baseline generation. This isolates syscall traffic per stage, not physical writes: `strace` omits mmap dirty-page writeback and distorts timing. These figures are a local attribution-validation sample, not a complete write-amplification ratio or SLA.
- Follow-up CPU profiling on the same Shardline/Turso benchmark (696 files, 174,082 nodes, 218,694 edges; Linux x86_64, Rust 1.98.1) found repeated changed-delta membership scans in endpoint/provenance validation. A per-delta lookup index replaced those scans without changing validation behavior or public contracts. In paired single profiled runs, initial Penelope publication moved from 105.1 s to 81.2 s, while incremental publication remained effectively flat (130.3 s to 128.8 s); treat these as directional single-run measurements, not a stable speed claim. The post-change profile reduced the membership-check symbol from 8.42% to ~0.02% of sampled CPU; remaining samples were distributed across Turso page-copy/B-tree/statement work. Profile data is retained in ignored `target/benchmark-results/profile-turso-shardline-20260928/` and `target/benchmark-results/profile-turso-shardline-20260928-after/`. The follow-up entry below splits the remaining Penelope publication bucket; elapsed time and retained database size are not write-amplification measures.
- Internal Penelope stage timing then localized the Shardline publication cost to Turso's graph transaction: initial graph commit was ~79.7 s versus 0.67 s record read/prepare/CAS and 1.17 s completion; incremental graph commit was ~130.3 s versus sub-millisecond record preparation and ~5 ms completion. Turso transaction-stage timing showed the fresh schema lacked temporal source/target indexes because bootstrap starts at schema v14 and skips the older v6-to-v7 index migration. A new checksummed v17-to-v18 migration now creates/rebuilds both indexes in endpoint-first order. Query-plan inspection also showed a combined `source_id OR target_id` predicate choosing the broad fact-kind/time index, so removed-edge lookup now uses two explicitly indexable `UNION ALL` branches; endpoint closure uses separate source and target updates. The previous per-removed-node query loop was then replaced by bounded 128-node batches. On the same single-run Shardline/Turso workload, before/after batching incremental indexing moved from 129,754 ms to 636 ms; the transaction's history/delta-mutation stage fell from 125.3 s to 12.2 ms, while temporal fact updates took ~176 ms. The tested delta removed 202 nodes and 298 edges; a conformance regression removes 262 nodes across multiple query batches and checks both prior and resulting historical snapshots. The final index adds about 28 MB to retained Turso storage in this sample (921,509,992 versus 893,624,424 bytes); this is a measured space/time tradeoff, not an I/O amplification or SLA claim. Raw local benchmark output remains under ignored `target/benchmark-results/`.
- Reusing Turso prepared statements for temporal fact closure/insertion and current canonical row mutations reduced one Shardline/Turso initial-index sample from 95,405 ms to 59,059 ms; incremental indexing remained effectively flat at 636/535 ms around this change. The temporal-write stage fell from 57.6 s to 22.5 s and current-row projection from 13.7 s to 7.9 s, while persistent-root timing varied from 4.3 s to 8.0 s, so the total remains directional single-run evidence rather than a repeatable speed claim. The optimization follows the existing SQLite adapter's statement-based batch structure and uses Turso's reusable `Statement` API; it does not alter transaction or publication semantics.
- SQLite snapshot restore now retains the canonical tree/cache built during its existing root-integrity check, allowing the next generation on a reopened reference store to use incremental tree mutation instead of rebuilding the full root. Three isolated Shardline SQLite incremental runs (696 Rust files, 174,082 nodes, 218,694 edges), each copied from the same generation-1 database and opened in a fresh process, measured 3.98–4.03 s total (median 4.03 s), with candidate application at 3.38–3.43 s (median 3.41 s) and durable persistence at about 0.29 s. The one-run pre-change baseline was 5.37 s total/4.90 s candidate application, so the before/after samples are not equally repeated. Candidate-clone time rose to ~153 ms because the retained cache is cloned. A separate three-run full-process benchmark, which keeps the handle open between generations and therefore exercises a different cache lifecycle, measured 5.045 s median incremental time and 4,001 MiB peak RSS; do not compare that directly with the reopened-store stage. This validates the targeted reduction but leaves the cache's memory tradeoff for further measurement. Canonical roots and persisted formats are unchanged.
- The Shardline-style repeated repository benchmark bundle now captures existing Penelope and Turso internal publication-stage timers into per-run JSON and median summaries, while retaining the raw stdout/stderr and fixture metadata. A one-iteration, two-file `syntaxmesh-core` harness validation (36,965 source bytes; 281 nodes; 303 edges) reported Turso initial temporal-fact writes at 25.0 ms of a 45.1 ms graph-commit stage; incremental temporal writes were 0.5 ms of 2.5 ms. This confirms instrumentation capture and stage ordering only; it is not representative-repository performance evidence. The subtimings are nested (`turso` is within Penelope `graph_commit`) and are not added together. Rerun on Shardline with multiple iterations before selecting another optimization.
- Three isolated Shardline runs (Rust 1.98.1, Linux x86_64, AMD Ryzen 9 7950X, 696 Rust files / 14,669,297 source bytes / 174,082 nodes / 218,694 edges) now provide stage medians with generation-root/reference validation after indexing. InMemory, SQLite, and Turso initial/incremental totals were 18.485/12.388 s, 36.207/23.926 s, and 52.210/0.675 s. Turso initial Penelope `graph_commit` was 41.066 s: temporal fact versions 20.581 s, current projection 7.052 s, persistent root 4.314 s, lineage/consequence projection 3.915 s, commit 2.616 s, and remaining measured stages ~2.5 s combined. Its incremental graph commit was 444 ms. SQLite's internal transaction stages are not yet attributed, so its 23.926 s incremental total cannot be assigned to a specific write path. Evidence bundle: ignored `target/benchmark-results/repository-20260928T135240.342986Z-uncommitted/`. The next evidence step is to stage-profile SQLite commit internals against its existing prepared-statement implementation and compare the same three-run fixture; do not infer that its canonical in-memory reference design should be replaced based on this timing alone. These are local medians, not an SLA or write-amplification result.
- SQLite now has optional `benchmark-instrumentation` stage timers following the Turso adapter's local profiling pattern, split across incident-edge cascade discovery, transaction/history/tree preparation, temporal facts, persistent roots, current projection, history/consequence/acceptance projections, checkpoint writes, and transaction commit. Instrumentation is off by default and changes no query or write behavior. All 33 SQLite tests pass with the feature enabled; a one-iteration all-backend benchmark on the two-file `syntaxmesh-core` fixture parsed and reported the SQLite timings successfully. The full Shardline median above predates this instrumentation; rerun the same three-sample fixture to attribute SQLite's large initial and incremental publication buckets before tuning.
- SQLite profiling on the repeated Shardline fixture then showed an unmeasured pre-transaction candidate graph update plus expensive temporal SQL. Wrapper timing confirmed candidate-store cloning itself was small (~0.085 s incremental), while applying the candidate delta took ~12.0 s; the adapter transaction then took ~11.4 s, including ~5.97 s preparing tree mutations and ~5.88 s writing temporal fact versions. Initial indexing had similar, smaller stages: candidate delta ~7.51 s, transaction ~11.75 s, temporal writes ~5.45 s. Following Turso's existing reusable prepared-statement pattern, SQLite now prepares and reuses the fact-close, incident-edge-close, and fact-insert statements for a temporal batch (also used by snapshot seeding). In one targeted Shardline before/after sample, initial temporal writes moved from 5.45 s to 3.64 s and total indexing from 30.12 s to 28.52 s; incremental temporal writes were 5.61/5.88 s and total indexing 23.61/24.19 s. This is directional single-run evidence with little/no incremental gain, not a performance claim; candidate-delta application and tree-mutation preparation remain the next separately measured costs. Full `cargo make ci` passes after the change, including workspace tests, all eight durable backend-conformance tests, strict Clippy, dependency checks, and API docs. Do not add the nested stage timers together or infer an SLA/write-amplification result.
- A function-level CPU profile of the same SQLite/Shardline benchmark attributed 2.93% of sampled process CPU to `BTreeMap::retain` in node-removal edge cascading: the old loop rescanned the full edge map for each removed node even though the in-memory incoming/outgoing edge indexes were already current. The cascade now removes only the incident edge IDs from those indexes. Existing query/reference semantics are unchanged; store tests and all eight cross-backend conformance scenarios pass. A targeted single sample moved incremental candidate-delta time from 12.02 s to 10.57 s and SQLite total incremental indexing from 23.98 s to 22.24 s; initial total was effectively flat at 28.49/28.34 s. The function-level profile is under ignored `target/benchmark-results/profile-sqlite-shardline-20260928/`; this initial timing comparison was diagnostic only. Remaining candidate-delta work includes full canonical-root computation and index rebuilding.
- SQLite persistent-tree mutation now follows the already-tested Turso endpoint-query approach: removed nodes are processed in bounded batches of 128, source and target lookups are separate `UNION ALL` branches explicitly bound to the existing endpoint indexes, and payload IDs/endpoints are validated against the removed-node set. The previous per-node `source_id OR target_id` query was replaced without schema or contract changes. A conformance test removes 260 nodes (>2 batches), then verifies pre-removal and current SQLite/Turso snapshots. Three fresh-database Shardline SQLite runs (same 696-file fixture, Rust 1.98.1/Linux/Ryzen 9 7950X) measured 33.805 s initial / 16.467 s incremental medians, versus 36.207 s / 23.926 s in the preceding three-run bundle. In the new bundle, incremental tree-mutation preparation is 2.3 ms (was ~5.9 s); candidate-delta application remains 10.4 s and temporal fact writes 5.7 s. Evidence: ignored `target/benchmark-results/repository-20260928T142930.178694Z-uncommitted/`. These are directional local medians, not an SLA or write-amplification result.
- The reference candidate path now follows the accepted schema-v2 root design after bootstrap: it retains the shared persistent fact-tree cache, applies only delta facts, and updates node/name/owner/adjacency indexes for changed identities rather than rebuilding the full indexes. It avoids cloning the page cache during mutation and drops the cache's duplicate dirty-page set, while the existing outer candidate publication preserves rollback semantics. Lineage validation now validates `ChangeSetDelta` independently (without re-validating the previous generation's potentially huge graph delta); event-reference checks derive deterministic event IDs from retained deltas rather than reconstructing the historical graph. A full-snapshot-root oracle test covers an incremental node removal with its incident-edge cascade. Three fresh-database Shardline SQLite runs on the same host/toolchain measured 31.939 s initial / 9.533 s one-file incremental medians, versus 33.805 s / 16.467 s immediately before these candidate-path changes. Candidate graph-delta time fell from 10.405 s to 3.409 s, and incremental persistent-root time was 89 ms; temporal fact writes remain the dominant measured adapter stage at 5.568 s. Peak process-tree RSS median was 4,002 MiB and retained SQLite pages were 769,024,000 bytes. Evidence: ignored `target/benchmark-results/repository-20260928T145908.898744Z-uncommitted/`. Comparison is directional across local runs, not an SLA or write-amplification result. Remaining high-cost work is incremental temporal fact persistence and the remaining candidate delta/indexer path; continue profiling those rather than broadening the backend architecture speculatively.
- SQLite temporal edge closure now follows the existing Turso endpoint-indexed approach: the former per-removed-node `source_id OR target_id` update is replaced with separate source/target `IN` updates over bounded batches of 128 removed nodes, reusing one prepared statement per endpoint within the transaction. The >128-node SQLite/Turso cascade fixture verifies current and historical snapshots remain correct. Three fresh-database Shardline SQLite runs measured 32.161 s initial / 4.158 s one-file incremental medians; incremental temporal-fact stage time was 166 ms versus the prior 5.568 s, and total incremental time was 9.533 s before this change. The result is a local directional comparison, not an SLA or write-amplification claim. Evidence: ignored `target/benchmark-results/repository-20260928T150756.497829Z-uncommitted/`. The next measured targets are remaining candidate graph-delta work (~3.43 s), initial-build temporal writes (~6.71 s), and initial persistent-root publication (~2.39 s).
- Shardline-scale SQLite evidence identified unnecessary history reconstruction during empty consequence publication: validation rebuilt active consequence/event state even when there were no consequence additions or retractions. The reference store now validates that the target generation exists and returns immediately for an empty `ConsequenceDelta`; non-empty deltas keep the prior full validation path. A regression test publishes a no-op consequence generation after an explicit consequence generation. Three fresh-database Shardline SQLite runs before/after measured median initial/incremental indexing at 32.889/4.241 s and 31.162/2.558 s; the incremental `candidate_graph_delta` stage moved from 3.450 s to 0.846 s, while retained database size stayed at 777,555,968 bytes. This same-host local result supports the specific no-op optimization but is not an SLA or write-amplification measurement. Evidence bundles: ignored `target/benchmark-results/repository-20260928T215533.876790Z-uncommitted/` (before) and `target/benchmark-results/repository-20260928T220111.943599Z-uncommitted/` (after).
- Current SQLite projection writes now prepare each file/provenance/node/edge statement once per publication transaction and reuse it across the changed facts, following Shardline's local-SQLite prepared-statement pattern and the existing SyntaxMesh temporal-fact writer. The three-run Shardline SQLite fixture measured initial current-projection time at 2.658 s versus 3.276 s before this change (~19% lower); overall initial indexing was 30.850 s versus 31.162 s (~1% lower), so the stage-local improvement did not materially change total initial time in this run. The one-file incremental median was 1.686 s versus 2.558 s in the prior bundle, but most of that gap is outside this small projection stage and should be treated as run variation. Retained database size remained 777,555,968 bytes. Evidence bundles: ignored `target/benchmark-results/repository-20260928T220111.943599Z-uncommitted/` (before) and `target/benchmark-results/repository-20260928T220647.876234Z-uncommitted/` (after). This does not measure write amplification or establish an SLA.
- A bounded 100-row multi-value SQL insert experiment for new temporal facts was rejected and removed: on the same three-run Shardline SQLite fixture, initial temporal-fact time increased from 6.478 s to 10.607 s, and initial indexing from 30.850 s to 34.614 s; incremental indexing was effectively unchanged (1.686/1.641 s). The existing single-row prepared statement remains the production path. The experiment's evidence is retained under ignored `target/benchmark-results/repository-20260928T221254.320923Z-uncommitted/`; this rules out that particular batching shape, not all future bulk-write designs.
- Optional SQLite benchmark instrumentation now separates temporal payload encoding, prepared-statement execution, and closure work without changing the persisted path. Three Shardline runs measured initial temporal-fact publication at 6.339 s median: payload encoding was 68 ms, SQL insert execution 6.234 s, and close operations 0 ms (~98% of measured fact-write time was statement execution). Overall initial/incremental SQLite indexing measured 30.408/1.670 s. Encoding is therefore not the next optimization target; investigate index/write cost or a different SQLite bulk-loading strategy, since the tested 100-row multi-value statement regressed. Evidence: ignored `target/benchmark-results/repository-20260928T221952.280439Z-uncommitted/`.
- Fresh SQLite-only Shardline evidence run (2026-09-28T23:53Z; rustc 1.98.1, Linux x86_64, AMD Ryzen 9 7950X; three isolated fresh databases) measured 31.602 s initial / 1.887 s one-file incremental medians, 3,605.9 MiB peak RSS, and 815,783,936 retained bytes. The fixture remained 696 Rust files / 14,669,297 source bytes but now indexes 188,965 nodes / 232,881 edges; this supersedes prior counts of 174,082 / 218,694 because the Rust pack now emits typed `use` facts, so older timings are not strictly like-for-like. Stage medians: initial candidate graph delta 3.226 s and SQLite transaction 16.472 s (including 5.015 s temporal SQL writes, 2.939 s persistent-root work, 3.056 s current projection, and 2.002 s commit); incremental candidate graph delta 998 ms versus a 269 ms transaction (15 ms temporal SQL, 101 ms persistent root, 10 ms current projection, 108 ms commit). A one-run isolated incremental CPU profile showed BLAKE3, JSON serialization of stable IDs, persistent-tree allocation, and memory/page faults among the sampled costs, but also included the benchmark's post-index full root/reference validation, so it does not attribute those samples solely to candidate application. Raw repeat bundle is `target/benchmark-results/repository-20260928T235326.663187Z-uncommitted/`; staged profile artifacts are under `target/benchmark-results/sqlite-stage-profile-20260929/`. Next, isolate subphases inside the shared candidate delta application (including its root/cache/index work) before optimizing; do not infer from the mixed CPU profile or these local medians an SLA or write-amplification result.
- Keep one writer coordinator while parsing/resolution remain parallel; gate Turso experimental MVCC behind capability tests.

Exit: an accepted generation survives restart and answers identical queries; interrupted or rejected writes leave the last published generation intact.

- Bounded, cursor-paged `accepted_between(from, until, after, limit)` is now exposed as a typed temporal query and CLI operation. SQLite/Turso use the ordered acceptance-time index; reference stores conform through their retained-history path. This is an acceptance timeline only, not `StateAtTime` or bitemporal graph reconstruction; see [ADR-0032](adr/0032-indexed-acceptance-timeline-query.md).
- Generic typed `FactHistory` now covers persisted files, provenance, nodes, and edges. Each version exposes generation-validity bounds plus distinct optional producer observation and engine acceptance timestamps; unknown legacy times stay unknown. Durable stores query the existing fact identity/validity index; File/InMemory use retained deltas as reference implementations. Shared cross-backend, CLI parsing, and DTO fixtures cover replacement, deletion, and reopen. This does not imply semantic ChangeEvent/consequence lineage; see [ADR-0033](adr/0033-typed-history-for-canonical-facts.md).
- `Query::fact_lineage` and `fact-lineage[-turso]` emit typed `FactSupersedes` links between contiguous versions of the same stable fact identity. Durable `ChangeEvent` payloads and changed-fact membership are published atomically and indexed; queries support bounded per-fact pages and direct generation lookup, including no-op transitions. Memory/File derive expected answers from history. Same-delta replacements and implicit edge cascades follow accepted-delta semantics; DTO, cursor, cross-backend, migration/backfill, no-op, and restart fixtures cover the slice. Temporal NDJSON is schema v10 after [ADR-0045](adr/0045-consequence-neighborhood-ndjson.md) added typed consequence edge/footer records and `consequence-neighborhood[-turso]` CLI output. Under [ADR-0038](adr/0038-indexed-change-event-correlation.md), a bounded query links later events that changed the same selected fact identity and labels it historical correlation, not cause. The first explicit ChangeSet membership query is fixed-generation and returns only explicitly assigned membership plus its asserting provenance. Under [ADR-0042](adr/0042-explicit-consequence-edges.md), evidence-backed consequence assertions publish atomically in File/SQLite/Turso and flow through Penelope's digest-bound v4 journal and the Engine API; v1–v3 Penelope records remain replay-compatible. SQLite v13→v14 and Turso v14→v15 add indexed temporal edge/evidence/parent schema, with SQLite's additive migration registered and rollback-tested. [ADR-0043](adr/0043-indexed-consequence-endpoint-reads.md) adds fixed-generation, cursor-paged endpoint reads; [ADR-0044](adr/0044-bounded-consequence-neighborhood.md) adds deterministic bounded weakly connected BFS through the pinned-generation query API. Durable stores use temporal endpoint indexes; File/InMemory serve as reference implementations. Fixtures cover restart/retractions, exact fact-version lookups, pagination, BFS bounds/truncation, and injected consequence-add/retraction rollback in SQLite/Turso. The acceptance-cutoff query described in ADR-0047 is implemented separately from generation-validity and producer-observation axes. Remaining: transitive propagation, representative storage/write-amplification measurements, and ChangeSet authoring access.

- [ADR-0048](adr/0048-cli-changeset-history-queries.md) exposes existing generation-pinned `ChangeSetAt` and cursor-paged `EventsForChangeSet` queries in File/Turso CLI commands. A cross-backend CLI fixture verifies declaration and event output plus cursor continuation. Authoring remains separate; the CLI does not infer or mutate ChangeSet membership.

### Gate 1 benchmark follow-up — representative storage and write traffic

- A three-sample Shardline run (Rust 1.98.1, Linux x86_64, Ryzen 9 7950X; 696 Rust files, 14,669,297 source bytes, 174,082 nodes, 218,694 edges) measured median initial / one-file incremental indexing at InMemory `19.599/3.470 s`, SQLite `32.630/4.199 s`, and Turso `54.873/0.658 s`. Median retained SQL database sizes were SQLite `769,024,000` bytes and Turso `921,509,992` bytes; peak process-tree RSS was `3,188.7/4,002.1/2,364.6 MiB` for InMemory/SQLite/Turso. SQLite `dbstat` attributes most retained pages to temporal tree pages (~177.5 MB), fact versions (~103.2 MB), history (~65.7 MB), checkpoints (~65.6 MB), and current nodes/edges. Turso has the same large temporal structures and retains ~20% more bytes than SQLite in this run. This is one repository and local hardware, not a release SLA.
- A separate single traced pass counted successful writes to each DB/WAL/SHM path: initial SQLite/Turso `6,366,100,328/9,725,203,200` bytes; one-file incremental `41,568,200/60,658,496` bytes. Initial DB/WAL/SHM splits were SQLite `768,999,424/5,597,100,568/336` and Turso `921,686,016/8,803,517,184/0` bytes; incremental splits were SQLite `18,743,296/22,824,888/16` and Turso `21,848,064/38,810,432/0`. The initial pass is WAL-dominated; this is a profiling clue, not proof of a specific cause. Latest serialized history payloads were `65,563,778` bytes for initial indexing and `98,560` for the incremental delta; syscall-bytes/history-payload ratios were `97.1x/148.3x` initially and `421.8x/615.4x` incrementally. These ratios are diagnostic proxies, not complete write amplification: `strace` distorts execution and omits mmap dirty-page writeback, device writes, and other persisted records. Raw evidence is under ignored `target/benchmark-results/repository-20260928T154431.968543Z-uncommitted/` and `target/benchmark-results/write-syscalls-20260928T155652.474302Z-uncommitted/`. The harness now reports the DB/WAL/SHM split, resolves child-process paths consistently, bounds printed syscall payloads, and creates disposable DB fixtures under ignored workspace scratch by default to avoid a nearly full `/tmp` quota.
- Shared store-delta stage timing is now feature-gated and emitted only when the benchmark enables `benchmark-instrumentation` plus `SYNTAXMESH_STORE_PROFILE`; the normal engine build has no profiling code. A three-run SQLite-only Shardline rerun (Rust 1.98.1, Linux x86_64, Ryzen 9 7950X; 696 Rust files / 14,669,297 source bytes) measured initial / one-file incremental medians of `31.712 s / 1.944 s`, `3,606.9 MiB` peak RSS, and `815,783,936` retained DB bytes. Nested initial/incremental `apply_delta_to_memory` persistent-root stages measured `1,442 ms / 48.688 ms`. For the incremental publication, the two in-memory candidate clones measured `197 ms` in the SQLite adapter and `352 ms` in the shared lineage layer; shared graph-delta work between those marks was `51.779 ms`, and SQLite transaction persistence was `336 ms`. The timings overlap by design and are not additive. This identifies repeated whole-state candidate cloning as a strong optimization candidate, not yet a proven fix; an owned-candidate API would affect the store-port contract and needs an ADR plus atomic-failure/conformance coverage before implementation. Raw evidence is under ignored `target/benchmark-results/repository-20260929T001451.271582Z-uncommitted/`.
- The legacy temporal-snapshot seeding path used when upgrading an older database now prepares its close/insert statements once and reuses them for each fact. This path is separate from normal initial indexing; Shardline benchmark timings do not measure this migration-only change. It preserves migration backfill semantics and atomicity; no speedup or write-amplification claim is made.
- First-generation publication has no prior temporal versions to close, but both adapters previously issued a no-match `UPDATE` before every inserted fact. SQLite and Turso now skip that update only when `GraphDelta.expected_base` is `None`; later generations and migration replay still close prior intervals. The Shardline sample's initial generation avoided 394,168 unnecessary close statements. Three post-change Turso initial-index samples were `38.949/38.484/38.745 s` (median `38.745 s`) versus the pre-change three-run median of `54.873 s` (~29% lower); all produced 696 files, 174,082 nodes, and 218,694 edges. This is a same-host local comparison, not an SLA. The adapter and shared temporal-history tests pass; write-amplification is unaffected because the skipped updates matched no rows.
- [ADR-0079](adr/0079-owned-candidate-publication.md) removes the measured nested whole-state clone while preserving isolated-candidate → durable commit → in-memory publish ordering. On the same Shardline SQLite benchmark (Rust 1.98.1, Linux x86_64, Ryzen 9 7950X, three fresh DB runs), median one-file incremental indexing moved from `1,944 ms` to `950 ms` (~51% lower), and SQLite's nested `candidate_graph_delta` stage moved from `902 ms` to `51 ms`. The redundant shared-store clone (`352 ms` before) is absent afterward; the required adapter candidate clone remains ~`204 ms`. SQLite persist was `363 ms` after versus `336 ms` before, so persistence itself did not get faster. Initial indexing moved `31.712 s` → `29.384 s`; peak RSS was `3,606.9` → `2,260.2 MiB`, but these are observed deltas, not isolated causal effects. Evidence: `target/benchmark-results/repository-20260929T001451.271582Z-uncommitted/` and `target/benchmark-results/repository-20260929T002848.379403Z-uncommitted/`. This is one local repository/host comparison, not an SLA.
- [ADR-0080](adr/0080-structurally-shared-fact-page-cache.md) changes only the in-memory immutable fact-page cache to a structurally shared ordered map. In three fresh-database Shardline SQLite runs, median one-file incremental indexing was `711 ms` versus `950 ms` in the preceding three-run baseline; initial indexing was `29.171 s` versus `29.384 s`. Candidate store cloning fell from `193.387 ms` to `120.398 ms`, with the persistent-page-cache clone rounded to `0 ms`. Retained DB bytes were unchanged; peak RSS was `2,288.4 MiB` versus `2,260.2 MiB`. Evidence: `target/benchmark-results/repository-20260929T004301.496482Z-uncommitted/`. This one-host comparison supports the targeted cache change, not a general performance guarantee.

### Gate 2 — deterministic Rust ingestion

- Scan paths with ignore rules and BLAKE3 content identities. Detect new, changed, and removed files before parsing.
- A counting-extractor fixture verifies the indexer performs exactly one extraction per initial file, zero on an unchanged rescan, and re-extracts only the edited file; deletion removes only that file's owned facts without another extraction. A delayed-result race fixture verifies stale extracted deltas are rejected, their Penelope records become terminal instead of poisoning recovery, the winning generation remains queryable, and a later index can proceed.
- Rust extraction emits source-backed call references without resolving them. The standalone resolver resolves a unique exact/terminal-name match across indexed files; matching keys normalize token spacing and generic arguments for qualified generic impl methods without rewriting stored node names or reference evidence. Missing and ambiguous references remain typed graph nodes. Indexer fixtures verify adding a definition re-resolves a previously unresolved call without reparsing the unchanged caller and that a qualified generic impl method call resolves to its definition. Rust `use` trees now emit typed named/glob import source facts through the existing `ImportRecord`/`NodeKind::Import` path; groups and nested groups are flattened, while Rust path resolution, `pub use` visibility/re-export modeling, `extern crate`, lexical-scope, trait, macro, and compiler-assisted resolution remain open ([ADR-0077](adr/0077-rust-use-source-facts.md)).
- Rust import full-workspace check (2026-09-29): the Rust CLI indexed SyntaxMesh itself (174 supported files; 27,992 nodes / 32,846 edges). Export contained 1,653 Rust named/glob import occurrences. Reopened File-store status confirmed `Durable`, matching graph root, valid references, current freshness, and no unindexed/changed/removed files. The Rust producer version bump triggered re-extraction through the existing identity invalidation path; no storage migration was added.
- `syntaxmesh-scanner` provides ignore-aware, extension-selected text scanning, normalized and sorted relative paths, BLAKE3 content/file identities, and per-run traversal metrics. Fixtures verify extension filtering, Git-ignore behavior, ordering, and repeat-scan identity. The CLI uses the scanner rather than owning a second traversal implementation; embedded and CLI indexing share this boundary.
- Each language extractor now declares a producer namespace/version. Before skipping a byte-identical file, the indexer checks the current provenance identity for that file and re-extracts only when the selected producer identity changed; a regression proves producer-version changes invalidate unchanged source while same-version scans remain parser-cache hits. The CLI also includes the deterministic sorted CompositeExtractor mapping/version fingerprint in its generation ID. This closes the stale-facts hole where parser upgrades otherwise left unchanged files indexed by old semantics. See [ADR-0058](adr/0058-extractor-identity-and-reindex-invalidation.md).
- CLI generation identity is transition-aware after bootstrap per [ADR-0114](adr/0114-unique-cli-index-transition-identities.md): the first/content-compatible identity remains stable, while a new scan from an existing generation derives an ID from that base and the scan/configuration fingerprints. The current-generation history entry is fetched by exact ID, not by materializing all retained history. A→B→A therefore produces distinct parent-linked generations; repeating the current scan returns the existing generation without appending a no-op. This corrects snapshot-ID reuse in Penelope without changing core ID DTOs or durable schemas.
- Source extractor provenance IDs now include producer namespace, stable file ID, and content hash. This prevents identical bytes in different files from colliding while keeping each provenance record bound to its exact source file. Rust, Python, TypeScript, and JavaScript producer semantic versions are bumped so unchanged indexed files are lazily re-extracted through the existing identity-invalidation path; the cross-pack and indexer regressions are in [ADR-0059](adr/0059-file-scoped-extractor-provenance.md).
- Initial synthetic scanner baseline: `cargo bench -p syntaxmesh-scanner --bench traversal --locked` scanned 1,000 Rust files among 500 non-Rust and 500 Git-ignored generated Rust files. It yielded 1,502 walker entries; five optimized end-to-end scans measured 4.962 ms, 4.269 ms, 4.333 ms, 4.209 ms, and 4.288 ms (median 4.288 ms). Environment: Rust 1.98.1, Linux x86_64, AMD Ryzen 9 7950X. One local run with a warmed filesystem cache is a reproducibility baseline, not a performance claim; rerun on representative repositories before tuning.
- Recompute only affected file/resolution partitions. Apply one atomic `GraphDelta` per accepted generation through a Penelope-backed indexing workflow.
- Publish the generation manifest/root and optional StateChronicle verification result.
- Recover prepared Penelope publications before preparing the next delta, including a crash after graph commit but before workflow completion.

StateChronicle verification is supported from this phase and is explicitly
opt-in (`--verify` in the CLI); default publication reports `DURABLE` only.

Exit: unchanged rescan performs zero parser work; file deletion removes only owned facts; a delayed old parse result cannot overwrite a newer generation; crash/restart completes or safely reconciles the run.

### Gate 3 — one usable query path

- Build generation-tagged adjacency projection and symbol index. Search, node, neighbors, path, impact, export, and status CLI reads now use the shared `SyntaxMeshEngine` query/status API for file and Turso stores; the host reads the manifest only to bind an existing store to its repository/worktree scope during engine construction.
- Return compact source evidence, certainty, generation identity, and explicit unknown/unresolved markers.
- `Query::export_subgraph` and file/Turso CLI commands emit deterministic weak-neighborhood NDJSON bounded by hops, nodes, and edges, with truncation recorded in schema-v3 streams; complete-generation export remains available. Every graph/history record now labels its temporal query mode (`historical_conclusion`, `observation_timeline`, or `acceptance_timeline`); retrospective-current-semantics mode is reserved but not emitted. Neighbor, path, impact, subgraph, node, search, and file-owner reads are generation-scoped; Turso uses endpoint/name/owner indexes for these point and neighborhood reads, while the reference file store rebuilds ephemeral adjacency indexes after restart. Turso open validates indexed rows and streams the canonical root without restoring a graph-sized heap snapshot ([ADR-0024](adr/0024-turso-row-authoritative-storage.md)); complete-generation export/status materializes its explicitly requested output, and SQLite remains an in-memory reference/conformance backend. CLI-to-embedded equivalence is covered for the Rust fixture, while mixed-language CLI tests cover Python, TypeScript, and JavaScript routing; daemon equivalence remains future work.
- The CLI and embedded `SyntaxMeshEngine` now index the same mixed-language fixture and produce byte-identical deterministic NDJSON exports. The host-equivalence test also runs the embedded engine directly over `SqliteGraphStore`, then compares its export with both the InMemory engine reference and File CLI output. This proves the engine publication/query path is not coupled to the File/Turso hosts; it does not yet establish daemon equivalence or replace the broader backend-conformance tests.
- `syntaxmesh --help`, `syntaxmesh -h`, `syntaxmesh help`, and no-argument invocation print a successful, command-complete usage summary; the host integration test exercises all four entry points. Unknown commands point users to `--help`.

Exit: queries match store facts after edit/restart, traverse without whole-graph JSON load, and never mix generations. A caller can trace the evidence behind an impact result.

### Gate 4 — extension and operating model

- A consumer-style integration fixture uses only public extension SDK and engine APIs to submit a namespaced metadata fact; the engine validates and publishes it through Penelope, and the fact is queryable from the accepted generation. A malformed namespace payload is rejected before writing workflow records or graph state, returns an actionable typed validation diagnostic to its caller, and a different valid extension publishes successfully afterward. This proves validation-failure isolation only; panics/timeouts are not isolated by an in-process producer API, and durable retention of extension validation failures remains separate work.
- A separate, Cargo-excluded consumer workspace at `fixtures/out-of-tree-extension` now builds and runs using only public SyntaxMesh crate APIs. It publishes a namespaced fact and a runtime-protocol observation through the Engine and queries both back from the accepted generation. `cargo make extension-fixture` runs this black-box compatibility check, and the CI task includes it; workspace formatting also checks the excluded Rust fixture. This establishes downstream compile/runtime usability for the current API, not stability across future API versions or process-isolated extensions.
- Public runtime-protocol fixtures submit observations through the same engine batch API. The engine stores the unchanged schema-versioned envelope in a distinct `RuntimeObservation` node with `RuntimeObserved` provenance; File restart/export and Turso restart fixtures verify the payload survives, prior static facts remain intact, opaque subject/object IDs do not create graph edges, observation time remains queryable, and the opt-in StateChronicle chain verifies after reopening Turso. Subject/object resolution and observation retention remain open.
- Engine and CLI status expose read-only prepared/completed/rejected Penelope operation counts; a workflow-adapter fixture proves diagnostics do not recover or mutate records. Bounded `workflow-rejections[-turso]` pages expose durable typed terminal reasons and stale-base generation context from the same Penelope journal; legacy rejected rows remain explicitly unknown. Store-level cursor paging is ordered and bounded in File, SQLite, and Turso. Filtering rejected phases still scans the run-key journal in bounded batches (worst-case O(number of workflow records)); this does not claim constant-time rejection lookup. Status also recomputes the canonical graph root and validates provenance/edge references, reports both checks for embedded and CLI consumers, and exits non-zero when a loaded generation fails either check. Turso/SQLite fail closed during open/restore on their existing graph checks. Separate `integrity` and `integrity-turso` commands run full backend checks on explicit request; SQLite/Turso checks include physical database integrity plus checkpoint-to-history-manifest and checkpoint-to-persistent-root validation, with corruption fixtures proving damaged checkpoints are reported while root-backed graph reads still succeed. The File check validates decode and equality with the open snapshot, not physical pages. These checks do not prove crash durability, freshness, or external-input recoverability. The engine compares current caller-supplied file versions to the accepted inventory without runtime/filesystem coupling; CLI status optionally scans a scope-validated source root and reports added, changed, and removed supported-language files, exiting non-zero when stale. Without a source inventory, freshness is explicitly `not_checked`. Actionable extension-validation errors are returned directly to the submitting caller without persisting rejected payloads. Panics/timeouts are not isolated by the in-process producer API.
- CLI end-to-end coverage now asserts default File/Turso indexing reports `Durable`, while Turso `index-turso --verify` reports `Verified` and the status remains `Verified` after reopening the database. This verifies the opt-in StateChronicle path at the user-facing composition boundary as well as in adapter/engine fixtures.
- The language SDK now supplies the host-independent `CompositeExtractor` router: an engine can receive multiple language extractors and dispatch each source by normalized extension, rejecting empty/duplicate registrations and unsupported source paths. Unit fixtures cover case normalization, routing, duplicate-registration atomicity, and extensionless paths. Rust, Python, TypeScript, JavaScript, Bash, and documentation extractors are composed here; Bash emits static function/command facts and the documentation pack emits Markdown heading sections, plain-text document roots, and conservative local links to indexed documents and supported source files as specified in [ADRs 0073](adr/0073-bash-and-documentation-source-facts.md), [0074](adr/0074-resolve-local-markdown-document-links.md), and [0110](adr/0110-resolve-local-markdown-source-file-links.md). Source-file links resolve to exact indexed `File` nodes only; anchors and member-level source links are not interpreted. Supported source inputs are limited to Rust, TypeScript/JavaScript, Python, Bash, and Markdown/plain text; SyntaxMesh itself is implemented and executed in Rust, without launching analyzed-language runtimes. A Rust CLI end-to-end fixture now proves configured Node and Python profiles resolve imports across separate packages in one monorepo while leaving Rust imports untouched; this is a baseline fixture, not broad monorepo compatibility evidence. Broader symbol coverage, additional representative language corpora, and comprehensive monorepo coverage remain Gate 5 work. ECMAScript Node and conservative Python module-resolution profiles and durable outcome diagnostics are implemented as documented below. See [ADR-0056](adr/0056-static-language-extractor-registry.md).
- [x] Document cold backup/rebuild, migration precautions, and corrupted-index recovery for the current local stores in [the v0 operations runbook](OPERATIONS.md). It explicitly distinguishes source-rebuildable Rust facts from runtime/extension facts and documents that NDJSON is not currently restorable. A Shardline-inspired backend-conformance recovery drill now cold-copies closed SQLite and Turso databases with any WAL/SHM sidecars, then verifies physical integrity, durable workflow-record preservation, and current/historical graph states after opening the copies. This covers only cold-copy restore, not live backup or crash/power-loss recovery.

Exit: embedded and standalone hosts behave equivalently; an extension failure is isolated; operators can tell whether a generation is durable, verified, stale, or recovering.

### Gate 5 — v0.1 product completion

ECMAScript receiver-scope precision (2026-09-30): Oxc class/function boundaries
now distinguish a named method's `this` context from ordinary nested functions
and object methods. Arrows preserve lexical context; dynamic/anonymous receivers
remain unresolved instead of producing invented enclosing-class call edges.
Nested named classes use the existing full declaration path. Extractor version
10 invalidates unchanged affected files; no resolver, DTO, dependency, or schema
is added. The Sim class-method callback pattern and existing extractor frames
are reused, with focused TypeScript/JavaScript fixtures and the existing
File/verified-Turso host harness. The lifecycle scenario checks call retraction,
unchanged repeat, reopened state, and retained historical evidence. This is
syntax-level receiver precision, not runtime dispatch or compiler equivalence.
See [ADR-0166](adr/0166-ecmascript-this-receiver-scope.md).

Literal ECMAScript namespace-member uses now reuse the existing Oxc AST,
typed import facts, and exact export/star-barrel binding path. Both quoted keys
and optional quoted access retain exact source evidence, including escaped
names. No new resolver, public type, dependency, or migration is introduced;
extractor version 9 invalidates unchanged affected files. Focused TypeScript
and JavaScript tests use Shardline's separate test-module pattern, and existing
CLI barrel and Turso retained-history scenarios exercise literal occurrences.
This narrows the earlier computed-key exclusion to dynamic expressions; those
expressions, compiler semantics, JSX, and representative semantic coverage
remain open. See [ADR-0165](adr/0165-literal-namespace-import-members.md).

The ECMAScript extractor's original substantial unit-test suite now lives in
`oxc_extractor/tests.rs`, following Shardline's sibling-module layout and the
project navigation rule. Its eight existing tests and four nested literal-key
tests retain their module identities and helpers; extraction behavior and
public contracts are unchanged. All 17 ECMAScript tests and 20 CLI
host/monorepo tests pass after the move, as do strict crate Clippy, workspace
formatting, and architecture checks. This is layout compliance for this
extractor, not a claim that every large module in the workspace has been split.

Real-corpus upgrade recheck (2026-09-30): an isolated copy of the retained Sim
index accepted all 2,572 unchanged files, updating all 2,550 ECMAScript producer
records from version 8 to 9. Reopened freshness, references, graph root, and
File-store integrity passed; an unchanged repeat reused the same generation
without another completed workflow. Retained old-generation facts hash exactly
like the original index. The 326 namespace-member occurrences are unchanged;
177 added source-anchor nodes/edges belong to the independent documentation
extractor upgrade, not literal-key extraction. This follows Shardline's isolated
upgrade/repeat verification approach and establishes corpus compatibility, not
model quality, comparative latency, compiler binding, or broader literal-key
coverage. Evidence and limitations are recorded in
[ADR-0165](adr/0165-literal-namespace-import-members.md).

- Shared semantic-evidence lifecycle (2026-09-30): one reused CLI scenario runs
  on File and explicitly migrated, StateChronicle-verified Turso. Two documents
  coalesce into one claim with two exact supports. Deleting the actual primary
  source rebinds the same concept identity to the surviving cached evidence;
  deleting the last source clears current semantic facts; restoring the survivor
  reuses its durable cache and reproduces its exact reviewed graph evidence.
  The mock listener is closed after initial extraction, and later runs use
  `--semantic-offline`. Unchanged survivor/empty runs preserve history length,
  original historical snapshots remain identical, and the final verified-chain
  audit passes. Shared host/HTTP/review helpers are reused; no production schema
  or public contract changed. This proves controlled cache/history lifecycle,
  not general entity resolution, cross-document inference, or live-model quality.

- Semantic replacement idempotence (2026-09-30): [ADR-0140](adr/0140-semantic-replacement-provenance-idempotence.md)
  fixes a reproduced cached-run history advance after model revision changes.
  Replacement compares desired provenance payloads without treating unrelated
  retained records as active semantic facts. CLI coverage checks zero inference
  and stable generation/history after a revision change; an embedded-engine
  fixture checks replacement, repeated clearing, and retained historical facts
  and provenance. The same CLI revision scenario now runs on explicitly migrated
  Turso with StateChronicle enabled: reopened statuses remain `Verified`, cache
  hits and rejected late model changes preserve history, and a full retained-chain
  audit passes. No provenance pruning, schema change, or new workflow was added.

- Cross-language segment-memo verification (2026-09-30): three fresh Turso runs per Sim/Shardline fixture after ADR-0137 retain 15/15 hits at both budgets. All 60 non-timing result rows and selected-input fingerprints match the preceding sibling runs. Current medians are Sim 462,035/1,066,388 µs and Shardline 294,881/766,521 µs at 2,048/8,192 tokens, roughly 53–72% lower in these selected fixtures. Exact tokens/items/omissions are unchanged. Raw bundles `context-20260929T221042.079201Z-uncommitted` and `context-20260929T221109.088290Z-uncommitted` are under ignored `target/context-retrieval-results/`. This supports the request-local exact-counter optimization across supported inputs, not whole-repository relevance or a latency SLA.

- Exact segment token reuse (2026-09-30): [ADR-0137](adr/0137-exact-context-segment-token-reuse.md) extends the pinned numeric-boundary proof to canonical context-item rank/span fields. Byte-identical segment counts are reused in a request-local cache capped at 256 KiB of keys and 1,024 entries. Both encodings pass full-encoding differential cases. Three fresh Rust-fixture runs retained 9/9 hits at both budgets with unchanged token/omission ranges; medians moved from 487,118 to 226,748 µs at 2,048 tokens and 1,359,044 to 458,709 µs at 8,192 tokens. Evidence: ignored `target/context-retrieval-results/context-20260929T220838.763058Z-uncommitted/`. These measurements do not close broader cross-repository relevance or latency gates; public context contracts and exact packing are unchanged.

- Mixed-input exact-counter verification (2026-09-30): the existing Shardline-style runner repeats fresh Turso retrieval after ADR-0136 on Sim and Shardline. Each repository returned 15/15 selected targets at both budgets across three runs. Sim covers TypeScript/JavaScript/Python/Bash/Markdown; Shardline adds Rust and an authored documentation-rationale paragraph. Median current query times were Sim 982,310/3,147,756 µs and Shardline 777,149/2,716,997 µs at 2,048/8,192 tokens. Evidence is retained in `context-20260929T220109.015317Z-uncommitted` and `context-20260929T220239.360367Z-uncommitted` under ignored `target/context-retrieval-results/`. This verifies fixed-target preservation across supported inputs, not whole-repository quality or a broader latency guarantee. Representative relevance/omission curves and larger-budget latency remain open.

- Exact tokenizer reuse (2026-09-30): [ADR-0136](adr/0136-exact-fixed-point-token-reuse.md) uses the pinned encodings' independent ASCII-number boundaries to reuse exact remainder counts across canonical JSON fixed-point passes that change only `token_count`. The memo is host/request-local and byte-bounded; noncanonical/oversized layouts fall back to full encoding. Both supported encodings are checked against full encoding. Three fresh fixture runs retained 9/9 target hits at each budget and unchanged token/omission ranges; median query times fell from 777,859 to 487,118 µs (2,048 tokens) and 2,227,114 to 1,359,044 µs (8,192 tokens). These are directional single-host fixture measurements, not broader performance claims. Multi-second context latency is reduced, not closed for representative repositories.

- Tokenizer reuse investigation (2026-09-30): inspection of the pinned `tiktoken-rs` 0.12.1 source found `CoreBPE::count_ordinary` delegates directly to `encode_ordinary(text).len()`, and `encode_ordinary_as` first materializes the same token vector. Switching to either API therefore does not remove the measured encoding/allocation work. A tentative switch was reverted; no tokenizer identity, budget semantics, or dependency changed. The optimization gate remains repeated exact encoding, not API renaming. Future approaches must retain the current full-payload counter as a differential oracle and demonstrate exact equivalence before adoption.

- Context tokenizer profiling (2026-09-30): test-only MCP counter instrumentation reuses the existing evaluation runner and records exact-tokenizer call counts/time separately from total query time. Three fresh Turso runs produced 18 query observations: 218–252 tokenizer calls per query accounted for 86.0–92.3% of measured query time. Raw `context_profile` rows are retained in ignored `target/context-retrieval-results/context-20260929T215214.457821Z-uncommitted/`. This identifies the dominant measured cost in this fixed fixture; it is not a general workload claim or permission to approximate token budgets. The next latency work should target repeated exact counting while preserving differential packing equivalence. Instrumentation is test-only and adds no public DTO or production profiling state.

- Context timing verification (2026-09-30): the Shardline-style runner executed without concurrent CI after ADR-0135. Three fresh Turso samples retrieved 9/9 fixed Rust targets at both budgets; median query times remained 780,801 µs at 2,048 tokens and 2,226,950 µs at 8,192 tokens. Raw evidence is in ignored `target/context-retrieval-results/context-20260929T214935.862768Z-uncommitted/`. These are current-state fixed-target measurements, not an isolated before/after comparison or broader relevance evidence. Multi-second context latency remains an open profiling gate.

- Exact context admission follow-up (2026-09-30): [ADR-0135](adr/0135-reversible-exact-context-admission.md) replaces whole accepted-pack clones per candidate with temporary append/count/admit and complete rollback. Differential tests retain the previous clone-based behavior as an oracle across budgets and Unicode/source items; tokenizer failure after a tentative count update restores the complete prior pack. Exact serialized-token accounting, ranking, and omissions are unchanged. This removes repeated accepted-text copies, not tokenizer work; latency improvements remain unclaimed without isolated measurement.

- Context source reuse (2026-09-30): [ADR-0134](adr/0134-query-local-verified-source-cache.md) applies Shardline SDX's shared decoded-block pattern to accepted source text within one context compilation. Matching file/content identities reuse provider reads and hash/UTF-8 validation; unavailable/stale/error results preserve per-node warnings and are retried by the next query. Successful retained text is capped at 8 MiB. Exact token packing is unchanged, so the documented multi-second query cost is not claimed solved; representative latency/relevance measurements remain open.

- HTTP-date retry follow-up (2026-09-30): [ADR-0133](adr/0133-http-date-semantic-retries.md) closes ADR-0132's date-header limitation using the crates.io `httpdate` parser in the CLI host. Fixed-clock tests cover past/equal dates, bounded future delays, the five-second boundary, and excessive delays. Durable scheduling and real-model evaluation remain open.

- Provider retry follow-up (2026-09-30): [ADR-0132](adr/0132-provider-directed-semantic-retries.md) copies Shardline SDX's provider-delay-before-backoff policy into the existing CLI adapter without its transfer stack. Integer `Retry-After` seconds are honored within a five-second synchronous wait bound; larger delays fail into existing retryable Penelope jobs. Focused HTTP fixtures cover zero-delay transient recovery, long-delay early exit, authentication rejection, and request/usage counters. HTTP-date delays and durable scheduling remain unsupported; real-model quality evaluation remains open because no local provider service is running.

- Offline semantic follow-up (2026-09-30): [ADR-0131](adr/0131-offline-semantic-cache-execution.md) reuses Penelope's durable per-document cache and retryable jobs for explicit `--semantic-offline` execution. It requires an asserted model revision, preserves cache configuration identity, and guards both metadata and inference HTTP paths. The one-command CLI fixture covers offline reuse and changed-document misses followed by successful online retry. Real-model quality and cross-document synthesis remain open.

- Semantic usage follow-up (2026-09-30): [ADR-0130](adr/0130-host-semantic-token-usage.md) adds host-only, checked parallel aggregation of provider-reported inference tokens. CLI integration coverage checks fresh inference, cache-only reuse, changed revisions, and usage retained when publication is rejected. Missing/malformed reports remain explicit and metadata calls are excluded. The source-of-truth semantic status now reflects the implemented one-command adapter; real-model quality, cross-document evidence synthesis, durable usage history, and monetary accounting remain open. Explicit offline-cache execution is implemented by ADR-0131 above.

- Automatic revision CLI evidence and endpoint follow-up (2026-09-30): the dedicated `semantic_revisions` process fixture exercises default-model automatic digest discovery through an ephemeral loopback endpoint, reopening the File store after each invocation. Initial/changed-model runs make one inference and three metadata requests; an unchanged run makes zero inference and two metadata requests without adding a generation. A changed digest updates semantic producer provenance while preserving the earlier snapshot; a revision change at the final publication check leaves the accepted graph/history unchanged. The CLI endpoint classifier now copies Shardline's OIDC `url::Host` matching pattern, correctly identifying IPv6 `::1` as loopback for both authorization/proxy policy and model-revision discovery. IPv4/IPv6/localhost positive and remote negative fixtures cover classification. The direct host-only `url` dependency reuses locked 2.5.8; core/DTO/protocol dependencies are unchanged. This is mock integration/consistency evidence, not real-model quality or latency evaluation.

- Semantic module organization (2026-09-29): following Shardline's separate test-module layout, the semantic contract's 258-line suite and Penelope semantic adapter's 428-line suite now live in dedicated `semantic/tests.rs` child modules. Implementation files retain only the test-module declaration. Existing test names, source/evidence validation, cache identity, restart, and partial-failure scenarios are preserved; this changes no public contract or stored representation.
- Host model-revision identity (2026-09-30): [ADR-0129](adr/0129-semantic-model-revision-identity.md) replaces alias-only CLI revision identity with automatic local Ollama digest discovery before cache lookup. Revision is checked after each inference call before cache completion and before publication. Other compatible providers require `--semantic-model-revision <immutable-revision>`; this caller assertion also permits offline cache reuse. Separate inference/metadata counts expose discovery overhead. Five host fixtures cover changed digest/cache-key identity, stable verification, missing/duplicate/malformed metadata, explicit offline policy, invalid options, and model changes during inference leaving the Penelope job retryable rather than caching output. Existing one-command cache/edit/history coverage now supplies an explicit fixture revision. This closes alias-only invalidation for honest reported local metadata; it does not provide model attestation, atomic inference pinning, or validation of caller-asserted revisions. Real-provider evaluation remains open.

- Semantic HTTP policy enforcement (2026-09-29): [ADR-0128](adr/0128-semantic-http-endpoint-policy.md) disables automatic redirects and loopback proxy discovery in the CLI provider. A transport fixture verifies an HTTP 302 remains a one-request failure; the one-command fixture succeeds against its local provider with unreachable HTTP/HTTPS/ALL proxy settings and no `NO_PROXY` override in the child environment. This prevents implicit transport routing from bypassing the configured local/remote endpoint policy.

- Explicit semantic prompt boundaries (2026-09-29): [ADR-0127](adr/0127-explicit-semantic-prompt-request-boundaries.md) corrects the CLI's flattened multi-document input. Packed prompts now preserve each independently cached request as a separate chunk group and instruct the model to keep a claim's evidence within one group. Prompt v2/hash invalidation prevents reuse of old-prompt results. The loopback one-command fixture checks two distinct one-chunk groups initially, one group after one document changes, and zero calls for an unchanged repeat. This aligns provider input with existing cache/evidence validation; it does not establish real-model quality or add cross-document synthesis.

- Coverage-guided source-extractor hardening (2026-09-29): [ADR-0126](adr/0126-fuzz-supported-source-extractors.md) copies Shardline's dedicated fuzz-package pattern into excluded `crates/fuzz`, with one Rust libFuzzer target selecting Rust, Python, TypeScript/TSX, JavaScript/JSX, Bash, Markdown, or text. `cargo make fuzz-source-smoke` uses disposable copies of the nine checked-in seeds and retains crash artifacts outside the corpus. Printable seed selectors were corrected to select their intended parsers. A 20-second local campaign on Linux x86_64 with rustc `1.99.0-nightly (7608eb7b0 2026-08-05)` and cargo-fuzz 0.13.2 completed 459,549 executions (~21,883/sec), adding 14,288 corpus units, with peak RSS 707 MiB and no reported crash. These are one-campaign panic/sanitizer results, not language completeness, exhaustive parser correctness, or a sustained fuzzing guarantee. Stable CI checks formatting and package isolation; longer campaigns remain separate.

- Markdown source links now reuse the existing source-reference graph: the Rust indexer materializes exact-path `File` facts only for linked, in-scope code files, resolves them through the existing unique-path resolver, and rechecks them when targets are removed. [ADR-0110](adr/0110-resolve-local-markdown-source-file-links.md) defines the boundary. The documentation extractor identity changes so incremental indexing re-extracts old Markdown files; no schema or storage migration is needed. The CLI fixture verifies indexed target resolution, missing-target retention, external-link exclusion, and removal/re-resolution. This is file-level navigation only; it does not resolve anchors to symbols.


Documentation content retrieval now includes verbatim Markdown paragraphs, code/HTML blocks, tables, and blank-line-delimited plain-text blocks. Each chunk carries exact source evidence and a `Contains` edge to the nearest section or document, making architectural context and decision rationale searchable while avoiding inferred prose claims ([ADR-0111](adr/0111-source-grounded-document-chunks.md)).

- Source-grounded semantic fact foundation (2026-09-29): [ADR-0115](adr/0115-source-grounded-document-semantic-facts.md) defines the provider-neutral contract; [ADR-0116](adr/0116-penelope-semantic-enrichment-cache.md) adds the Engine's explicit opt-in path and Penelope-managed cache/job record through the existing durable-record CAS port. [ADR-0117](adr/0117-document-section-context-for-semantic-input.md) includes authored section-heading paths in generation-pinned provider input and cache identity while exact quotes remain bound to chunk text. Prepared jobs resume after restart; completed output is rebound to exact current source spans. Keep inferred facts distinct from authored source chunks and source-file links.
- Bounded semantic document batching (2026-09-29): [ADR-0118](adr/0118-bounded-generation-semantic-batches.md) adds a generation-pinned Engine API for exact source chunks across documents, bounded by chunk count and UTF-8 bytes. Cross-document bounds, authored heading retention, and malformed containment have integration coverage.
- One-command semantic indexing (2026-09-29): [ADR-0119](adr/0119-one-command-parallel-semantic-indexing.md) adds an opt-in CLI adapter that indexes deterministic facts and then performs cached, bounded parallel semantic extraction in one command. [ADR-0124](adr/0124-default-project-local-index-command.md) removes both positional paths from the common invocation: from a repository root, `syntaxmesh index --semantic` uses the current directory and `.syntaxmesh/index.snapshot`. The default is local loopback Ollama-compatible HTTP; remote HTTPS requires explicit `--allow-network`. Evidence-validated results replace the current semantic namespace atomically, preserving graph history. A loopback mock integration test exercises the default-path command, unchanged cache reuse, and one-document invalidation. Semantic querying, cost telemetry, and actual model-quality/relevance evaluation remain future work; the CLI has not yet been validated against a real installed model. Without `--semantic`, indexing remains deterministic and provider-free.
- Document-granular semantic cache with multi-document prompts (2026-09-29): [ADR-0120](adr/0120-per-document-cache-with-multi-document-prompts.md) creates independently cached document requests in Engine/Penelope, then packs only cache misses into bounded multi-document CLI prompts (22 requests, 32 chunks, 48 KiB, four calls in flight). Loopback integration proves two documents share one model call, unchanged runs make zero calls/publications, and editing one document reuses the other document's cache and replaces obsolete current semantic claims. A Penelope partial-failure regression proves a completed document result survives a peer failure and only the still-prepared request is retried. Claims must be fully supported within one independently cached request; evidence spanning cache units is rejected, preventing stale cross-document inference after partial invalidation. Model-level relevance/latency evaluation and behavior for very large documents split across several cache units remain future work.
- Bare semantic opt-in (2026-09-29): [ADR-0121](adr/0121-default-local-model-for-semantic-indexing.md) aligns CLI help and parsing so `--semantic` uses the existing local Ollama default model `qwen3:latest`, while `--semantic <model>` remains an override. It does not auto-download model weights or start the external local model service. Without the flag, deterministic indexing remains provider-free.
- ECMAScript export binding (2026-09-29): [ADR-0122](adr/0122-ecmascript-export-binding-resolution.md) extends the existing Oxc module-resolution stage with deterministic exact-name binding for static named/default imports to unique explicit export occurrences. [ADR-0123](adr/0123-ecmascript-star-export-binding.md) adds named-import traversal through indexed `export *` chains: explicit exports shadow star branches, default does not traverse stars, cycles terminate, and multiple distinct results remain unbound. CLI monorepo coverage exercises two-hop TS path/index barrels, ambiguous branches, explicit shadowing, cycles, default exclusion, and binding retraction/restoration. A Turso restart/`GraphAt` fixture verifies historical star-chain module and binding edges survive barrel deletion. The graph retains both the resolved module and (when unique) export target using existing provenance and `ResolvesTo` history. No new dependency, DTO variant, table, or migration was needed. Static and literal namespace-import member uses are implemented (ADRs 0125 and 0165); compiler semantics, JSX, and broader language corpus validation remain open Gate 5 work.


- The sibling Sim TypeScript/JavaScript corpus exposed false syntax errors in Tree-sitter, so [ADR-0064](adr/0064-oxc-ecmascript-parser.md) moves ECMAScript parsing to crates.io Oxc 0.152.0; Python remains on Tree-sitter. The reproducible `parse_corpus` example parses all 2,550 ECMAScript files in Sim without diagnostics. Oxc emits typed source-occurrence records for imports and exports, preserving binding aliases, type-only status, exact binding spans, and static/dynamic/CommonJS syntax; the indexer persists them as typed `NodeKind` facts and module-ownership edges. [ADR-0066](adr/0066-import-export-as-typed-graph-facts.md) records this implementation, which reuses existing node history and requires no new storage migration. On 2026-09-28, the typed implementation indexed the full Sim tree: 2,557 files, 86,384 nodes, and 88,687 edges. Reopened File-store status reported current freshness, zero unindexed/changed/removed files, matching graph root, and valid logical references; deterministic export contained 18,102 typed Import nodes and 7,811 typed Export nodes. This supersedes the earlier 72,447-node corpus run, which predates typed lowering. Package/path resolution, unique exact-name named/default and static namespace-member export binding, and conservative indexed star-chain binding are implemented; dynamic namespace keys, JSX semantics, broader symbol forms, and validation against additional repositories remain open Gate 5 work. See [ADR-0125](adr/0125-ecmascript-namespace-import-member-resolution.md).
- Current CLI composition recheck (2026-09-29): `syntaxmesh index` scanned the existing Sim repository using the Rust CLI and default Rust-hosted composite extractors, then reopened the File store with `status`. It accepted 2,572 files and published 86,648 nodes / 88,929 edges; status reported `Durable`, matching graph root, valid references, current source freshness, and zero unindexed/changed/removed files. This corpus includes TypeScript/JavaScript, Python, Bash, and documentation inputs (Sim contains no Rust source); Rust is separately represented by the Shardline benchmark above. A diagnostic attempt on the mixed `bounty-swarm` sibling correctly stopped at `sei-chain/loadtest/scripts/validator_failures.py`, which has actual Python syntax errors (including a missing function colon). This is consistent with the fail-closed parser policy in [ADR-0064](adr/0064-oxc-ecmascript-parser.md): malformed input is not silently omitted or published as partial facts. The corpus failure is not evidence that Python runtime execution is required; all extraction was performed by SyntaxMesh's Rust code.
- Full CLI composition on Shardline (2026-09-29): the default Rust-hosted scanner/extractor stack indexed 810 supported files into a durable File-store generation, producing 179,771 nodes / 224,870 edges. Reopening the store with `status` confirmed matching graph root, valid references, current source freshness, and zero unindexed/changed/removed files. Shardline contributes the Rust corpus plus Python, Bash, and documentation inputs; combined with the Sim run above, the current CLI has now passed representative local-corpus ingestion for every in-scope language family and documentation inputs. These are correctness/integrity observations, not performance claims or broad language-completeness evidence.

- Namespace-member extractor corpus recheck (2026-09-29): with the new Oxc extractor and no module-resolution profile enabled, `syntaxmesh index /home/ac/projects/sim <isolated-snapshot>` scanned the clean Sim revision `63f18995d`, accepted all 2,572 supported files, and published 87,309 nodes / 89,590 edges. The deterministic graph export contains 326 `NamespaceMember` occurrences. Reopened status reported current source freshness, matching graph root, valid references, and zero unindexed/changed/removed files; File-store backend integrity passed. This is full-corpus extraction evidence only: unique export binding is separately covered by the CLI multi-hop/ambiguity/retraction fixture and the Turso restart/`GraphAt` test, not asserted across the Sim corpus. The isolated snapshot remains under ignored `target/validation/sim-namespace-20260929/`; the Sim worktree was not modified.

- Typed import/export extraction and persistence are implemented per [ADR-0066](adr/0066-import-export-as-typed-graph-facts.md): source events become typed graph nodes and reuse existing node history, indexes, roots, and queries, with cross-backend history coverage and no separate fact tables or snapshot fields. Full Sim typed indexing and integrity evidence now pass, and the resolver's terminal-name exclusion regression exists. Module resolution is specified by [ADR-0067](adr/0067-runtime-neutral-module-resolution-provider.md), with CLI composition in [ADR-0068](adr/0068-cli-module-resolution-profile.md). The provider contract, generic-filesystem Oxc adapter, optional Indexer/Engine injection path, and opt-in CLI Node profile are implemented. [ADR-0069](adr/0069-indexed-module-resolution-node-kinds.md) adds the explicit database-private node-kind projection and indexed Turso inventory query; SQLite and Turso migrations backfill and validate the same projection without changing canonical node payloads. [ADR-0070](adr/0070-versioned-module-resolution-diagnostics.md) retains unresolved, ambiguous, invalid, and non-indexed outcomes as provenance-backed temporal graph facts with generation-pinned query and CLI access. Focused tests cover migration backfill, Turso query-plan index use, indexed-target filtering, separate resolution provenance, target-removal and profile-disable re-resolution, source occurrence preservation, import/require conditions, config-sensitive generation identity, durable diagnostic lookup after restart, and Node-resolution persistence across a Turso restart with historical `GraphAt` retrieval through the CLI. Bundler and other profiles remain open.

- Context compiler implementation update (2026-09-29): the versioned query-layer pack now has deterministic generation-pinned selection and packing, and the separate read-only MCP host supplies a canonical-root-scoped source reader plus exact `cl100k_base`/`o200k_base` token counting. The `context` tool is exercised through both the MCP server API and stdio protocol; source hash mismatch remains a surfaced warning and path traversal/symlink escapes are rejected by the host. A Shardline-style evidence runner is available as `cargo make benchmark-context-retrieval`: it records host/toolchain/worktree metadata and a BLAKE3 fingerprint of the evaluation inputs (the current workspace has no commit yet), preserves raw logs and JSON rows, refuses to overwrite a run, and repeats the fixed evaluation against fresh temporary Turso databases. The fixture indexes the two Rust files in SyntaxMesh Engine and checks three known functions at 2,048- and 8,192-token budgets. Initial runs exposed missed targets at 2,048 tokens; splitting `_` in symbol names during lexical scoring and preferring a complete function-symbol match corrected that. The first three-run capture retrieved all targets in all samples (9/9 at each budget); median context-query latency was 766,485 µs at 2,048 tokens and 2,179,852 µs at 8,192, excluding fixture indexing. Packed token ranges were 1,989–2,037 and 8,077–8,141 respectively; omitted candidates remain explicit. The capture is stored under ignored `target/context-retrieval-results/`; this narrow two-file result is an iteration signal, not broad relevance evidence or a performance claim. The context gate remains open for representative repository relevance/omission curves, equivalence across all supported stores/hosts, and model-API prompt-envelope accounting.
- Context backend conformance follow-up (2026-09-29): the shared query compiler now emits equal `ContextPack` DTOs on InMemory, File, SQLite, and Turso for the same source-backed graph fixture; File/SQLite/Turso results remain equal after reopening their stores. The fixture also checks a complete source-evidence item, pinned generation, and budget bound. This establishes basic backend consistency, not representative retrieval quality or MCP-versus-embedded host equivalence; those remain open alongside model-API prompt framing.
- Context selector verification follow-up (2026-09-29): a source-backed retrieval fixture now checks tied lexical candidates produce an ambiguity warning, full source lines and one-based spans are preserved, repeated packs are deterministic, and constrained budgets report complete omitted candidates while keeping the serialized result within budget. This verifies those selector invariants on a focused fixture; ranking usefulness and omission tradeoffs on representative repositories remain open.
- Cross-language context follow-up (2026-09-29): the Shardline-style evidence runner now also selects named source targets from Shardline or Sim while using the same Rust `CompositeExtractor` registrations as the CLI. It indexes only the fixed target files (never runs the analyzed code), then evaluates the MCP context selector at 2,048 and 8,192 tokens. One fresh-database Shardline sample retrieved all four targets at both budgets (Rust webhook, Python metadata, Bash query, Markdown section); a Sim sample retrieved all five targets at 8,192 and four of five at 2,048 (TypeScript function, Python function, JavaScript module, Bash script, Markdown section). This demonstrates a real 2,048-token omission for the TypeScript target and successful larger-budget recovery, not broad whole-repository relevance or language-completeness evidence. It does not replace the separate full-repository indexing/integrity checks above. Raw runner artifacts are under ignored `target/context-retrieval-results/`.
- Documentation-content context follow-up (2026-09-29): the Shardline-style sibling retrieval evaluation now includes an actual rationale paragraph from `docs/benchmarks/README.md` as a `DocumentChunk` target, in addition to its Markdown heading target. The query asks why benchmark numbers are not claims across machines; the exact authored paragraph was returned with repository source evidence at both 2,048 and 8,192 tokens in three fresh Turso runs. The fixture deduplicates selected source files while retaining multiple independently evaluated targets. Input fingerprint: `cb7574e941c3f41479b279dd9823fea985301dece616324d5b213e5111899124`; measured context-query times for that paragraph ranged 1.489–1.528 s at 2,048 tokens and 5.202–5.482 s at 8,192 tokens. These are three local fixed-target samples, not corpus-wide relevance or latency claims. Broader retrieval quality remains open.
- Historical point-query follow-up (2026-09-29): [ADR-0084](adr/0084-runtime-neutral-historical-node-point-query.md) exposes the durable stores' existing indexed `historical_node` capability through the runtime-neutral query service, with `node-at` and `node-at-turso` CLI entry points. It avoids complete `GraphAt` materialization for one-node historic inspection; complete snapshots remain output-sized. A query-service test covers a node across removal, and the Turso module-resolution restart test now exercises the CLI point lookup after the current projection has deleted that node. Calendar-time selection and historical adjacency traversal remain separate gaps.
- Historical adjacency implementation (2026-09-29): [ADR-0085](adr/0085-temporal-adjacency-index-design.md) selects a nested persistent incidence map over delta replay, complete `GraphAt` materialization, temporal endpoint B-trees, or rewriting whole high-degree adjacency lists. The `syntaxmesh-store` Rust crate provides database-independent mutation/page primitives with tests for direction, bounded ordering, removal, root validation, structural sharing, multi-root page draining, and cold-load/resume on a 128-edge endpoint. SQLite publishes incidence roots with graphs and backfills them at migration v18→v19; Turso has matching atomic publication and ordered v21→v22 backfill. [ADR-0086](adr/0086-generation-pinned-historical-neighbor-pages.md) exposes bounded pages through `GraphStore`, the generation-pinned query service, versioned temporal NDJSON schema v10, and `neighbors-at[-turso]`. Tests cover bounded ordered continuation in both durable stores, cursor/query binding, and File/Turso CLI equivalence. Shardline-style opt-in counters ([ADR-0090](adr/0090-benchmark-historical-index-read-counts.md)) found that retrying after missing pages performed 182 persistent-tree cache lookups for a 10-edge page despite loading 18 pages. [ADR-0091](adr/0091-resumable-persistent-adjacency-reads.md) copies the resumable-cursor pattern to the read path: lookup counts fell to 47 at 64/256/1,024 generations and 59/69/69 across 100/1,000/5,000 graph nodes, with page loads and 10 fetched edge payloads unchanged. The subsequent statement-count run reports 30 SQL reads at all tested history depths and 36/41/41 across those graph sizes (10 edge rows each). These are direct fixture counts, not strict O(1) graph-size evidence; representative workloads and broader temporal traversal bounds remain open. The supported implementation/input scope remains Rust-hosted analysis of Rust, TypeScript/JavaScript, Python, Bash, and documentation text only.
- Initial historical-incidence build profiling (2026-09-29): SQLite instrumentation on the read-only Shardline repository showed that repeatedly path-copying incidence pages from empty roots dominated index construction. `PersistentIncidenceApply` now reuses each endpoint's in-progress nested root and publishes one outer-root update per direction/endpoint group for resumable incremental mutations. First-generation publication instead groups the sorted incidence facts by endpoint, bulk-builds each empty nested tree and then the outer endpoint tree using the existing `PersistentFactTree::apply(None, sorted_upserts, ...)` builder; SQLite and Turso select this only when there is no parent generation. The bulk builder produces the same content-addressed root as the incremental oracle, and an instrumented fixture verifies fewer cache lookups. Across three fresh SQLite runs, after outer-root batching cache lookups had fallen from 14,142,535 to 9,150,508; the sorted bulk build then reduced initial incidence-build cache lookups to zero. Compared with the immediately prior non-batch baseline, median incidence apply and reachable-dirty traversal fell from 10.03 s / 3.35 s to 1.45 s / 0.57 s; median initial indexing moved from 44.9 s to 32.0 s (about 29% lower), with retained database size unchanged at 975,605,760 bytes and median peak RSS moving from 4,420 to 2,250 MiB. A three-run Turso Shardline sample likewise measured median initial indexing at 47.5 s versus the earlier 73.0 s sample; retained database size remained 1,137,438,824 bytes, with peak RSS at 2,884 MiB versus the earlier 5,653 MiB. These are directional single-host results, not an SLA or isolation of the bulk builder's effect from other run variation. Evidence is retained in ignored `target/benchmark-results/repository-20260929T072910.960927Z-uncommitted`, `target/benchmark-results/repository-20260929T073529.258515Z-uncommitted`, `target/benchmark-results/repository-20260929T075806.360445Z-uncommitted`, and `target/benchmark-results/repository-20260929T080049.470006Z-uncommitted`. A separate bounded multi-row `INSERT` experiment for first-generation fact rows was rejected: its temporal-write median rose from 4.79 s to 13.50 s and total initial indexing from 44.9 s to 51.4 s; none of that experimental path remains. Raw evidence is in ignored `target/benchmark-results/repository-20260929T074509.398158Z-uncommitted`. Remaining SQLite write/dirty-page costs and write amplification still need profiling on more repositories.
- Bounded historical traversal follow-up (2026-09-29): [ADR-0087](adr/0087-bounded-historical-neighborhood.md) adds `Query::historical_neighborhood` over the pinned generation's incidence pages. It reuses deterministic weakly connected BFS and applies independent hop, node, edge, scanned-incidence, and (per [ADR-0089](adr/0089-byte-bounded-historical-neighborhood-output.md)) serialized-result-byte caps, returning stable node IDs, edges with shortest discovered depth, byte accounting, and explicit truncation. Focused fixtures cover cycle-edge deduplication, duplicate seeds, depth/node/edge/scan/byte limits, and missing-seed rejection. [ADR-0088](adr/0088-historical-neighborhood-stream-and-cli.md) adds an independently versioned node/edge/footer NDJSON stream and matching `neighborhood-at` / `neighborhood-at-turso` commands; [ADR-0089](adr/0089-byte-bounded-historical-neighborhood-output.md) caps serialized output at 16 MiB and records item-byte totals. An end-to-end fixture confirms deterministic File/Turso stream equivalence. Generation/calendar-range selection, relation/evidence filters, broader temporal axes, and operation-count instrumentation across full multi-hop traversals remain open. No additional source language or runtime is introduced.
- Consequence traversal work-bound follow-up (2026-09-29): [ADR-0092](adr/0092-bounded-consequence-neighborhood-work.md) reuses the historical-neighborhood bounds and indexed cursor reads for fixed-generation consequence BFS. It caps hops/endpoints/edges/scanned incidences, reports the scan count in the NDJSON footer (temporal schema v10), and pages high-degree endpoints instead of loading an arbitrarily large result. The separate cross-generation `ConsequenceTrace` is now implemented as described in [ADR-0093](adr/0093-generation-range-consequence-traces.md); fixed-generation BFS is not presented as temporal propagation.
- Turso current-projection write follow-up (2026-09-29): profiling on Shardline (696 Rust files; 188,965 nodes / 232,881 edges; rustc 1.98.1; Linux x86_64; Ryzen 9 7950X) separated row encoding from prepared-statement execution. Encoding the initial node/edge projection took about 121 ms combined, while executing the reused per-row statements took about 8.94 s. A Turso-only first-generation multi-value insert path now binds 100 nodes (700 parameters) or edges (500 parameters) per prepared statement; incremental generations retain the single-row prepared path. On three fresh Turso databases, median initial indexing moved from 48.633 s to 45.886 s, and the current node/edge projection stages moved from 9.086 s to 7.243 s (~20% lower). One-file incremental indexing stayed effectively flat (2.853 s to 2.798 s); retained database bytes stayed at 1,137,438,824 and peak RSS at ~2.88 GiB. Each measured generation passed the benchmark's full snapshot-root verification. This supports the bounded initial-load batch shape on this Turso workload only; it does not generalize to SQLite (whose analogous temporal-fact batch experiment regressed), establish an SLA, or measure write amplification. Evidence bundles: per-row baseline `target/benchmark-results/repository-20260929T082227.620098Z-uncommitted` and batched path `target/benchmark-results/repository-20260929T082930.858056Z-uncommitted`.
- Turso persistent-page write follow-up (2026-09-29): the now-separated root profile showed Shardline initial publication spending 1.43 s building the incidence tree, 5.33 s persisting incidence pages, and 4.76 s persisting canonical fact-tree pages; the incidence-root row itself took ~0.04 ms. Both page writers had prepared a new SQL statement for every page. They now prepare the existing `INSERT OR IGNORE` once per publication and reuse it for reachable dirty pages, preserving the page format, content IDs, and transaction boundary. Three fresh Turso runs reduced combined canonical+incidence root publication from 11.66 s to 7.69 s (~34%); incidence page persistence fell from 5.33 s to 2.85 s (~47%). Initial indexing was 46.852 s before and 45.600 s after; incremental remained around 2.9 s, retained DB bytes stayed 1,137,438,824, and median peak RSS was 2,882.7/2,881.0 MiB. Full root verification ran on each benchmark generation. Evidence bundles: split-stage baseline `target/benchmark-results/repository-20260929T084420.902020Z-uncommitted` and prepared-page path `target/benchmark-results/repository-20260929T084848.954395Z-uncommitted`. These measurements support prepared statement reuse on this Turso fixture; they are directional local timings, not an SLA or write-amplification claim.
- Turso persistent-page batch follow-up (2026-09-29): the prepared-statement path still issued one execution per dirty page. Both persistent-tree page writers now batch up to 100 `INSERT OR IGNORE` rows per statement (400 bound values, under SQLite's 999-variable limit), retaining the same page IDs, payloads, conflict behavior, and single publication transaction; small writes use the prepared single-row statement. Three fresh Shardline Turso runs reduced initial incidence-page persistence from 2.85 s to 2.28 s (~20%) and combined canonical+incidence root publication from 7.69 s to 6.77 s (~12%). Initial indexing moved from 45.600 s to 45.006 s, one-file incremental from 2.869 s to 2.914 s, retained database bytes remained 1,137,438,824, and median peak RSS remained ~2.88 GiB. Full graph-root verification passed for every measured generation. Evidence bundles: prepared-row path `target/benchmark-results/repository-20260929T084848.954395Z-uncommitted` and batched path `target/benchmark-results/repository-20260929T085647.559954Z-uncommitted`. This is a directional local result for the measured Turso workload, not an SLA or write-amplification claim.
- SQLite persistent-page batching was evaluated but not retained (2026-09-29): copying Turso's 100-row page inserts reduced the two initial page-insert stage medians from 1.391 s combined to 1.307 s (~6%), but whole initial indexing moved from 32.105 s to 34.903 s, incremental from 621 ms to 684 ms, and peak RSS from 2,250 to 2,292.6 MiB; retained database size remained 975,605,760 bytes. These three-run comparisons are noisy and do not prove the batch caused the end-to-end regression, but the targeted gain is too small to justify adding backend complexity without a clear overall win. SQLite therefore keeps its prepared one-row statement inside the existing transaction; the Turso optimization remains adapter-specific. Root verification passed for the benchmarked generations. Evidence bundles: single-row baseline `target/benchmark-results/repository-20260929T090403.690736Z-uncommitted` and experimental batched path `target/benchmark-results/repository-20260929T090923.404905Z-uncommitted`.
- Generation-range consequence tracing (2026-09-29): [ADR-0093](adr/0093-generation-range-consequence-traces.md) rejects union-over-time traversal and fixes chronological validity semantics. The store/query layers expose bounded endpoint pages over an inclusive generation range, carrying each assertion's start and exclusive end generation. SQLite/Turso query existing endpoint/validity projections directly; reference stores reconstruct intervals from retained consequence deltas. Cursor identity binds endpoint and both range bounds. `Query::consequence_trace` composes these pages into a bounded, outgoing temporal DAG keyed by endpoint/generation/depth states; each hop preserves its original typed relation, exact endpoints, evidence references, derivation, provenance, and validity interval, while assigned hop generations are non-decreasing. A delayed-hop/retraction-boundary fixture exercises the reference path, and a differential fixture compares the full trace across InMemory, SQLite, and Turso. [ADR-0094](adr/0094-temporal-consequence-trace-ndjson.md) adds independently versioned, byte-bounded query-layer NDJSON for the header, DAG states, hops, and footer, exposed by `consequence-trace` and `consequence-trace-turso`; a CLI integration fixture checks both store hosts. [ADR-0095](adr/0095-consequence-trace-evidence-class-filter.md) adds exact-set producer-provenance evidence-class filtering through the query API and optional `--evidence-classes` CLI flag; SQLite/Turso resolve only referenced provenance IDs through keyed bounded reads. Query-plan regressions assert both durable range-page queries use the source and target endpoint indexes. [ADR-0096](adr/0096-benchmark-consequence-range-read-counts.md) measures one fixed-result endpoint-range page at history depths 64/256/1,024: the reference path materialized/scanned generation and consequence history proportional to depth, while SQLite/Turso now fold generation-bound resolution and indexed-page retrieval into one SQL read at each depth. [ADR-0097](adr/0097-benchmark-composed-consequence-trace-work.md) carries opt-in counters through a typed two-hop trace on a fixed two-edge chain: InMemory materialized/scanned two pages' worth of retained generation/consequence history, whereas SQLite/Turso performed two page-level SQL reads plus two distinct generation-sequence lookups (four counted SQL calls total for this no-provenance-filter fixture) at every tested depth; all stores returned two hops and three scanned incidences. Query-local memoization avoids re-reading duplicate generation IDs. This single-host synthetic fixture is not an O(1) claim or a complete statement count for arbitrary traces. The trace is historical evidence, not an inferred causal claim.

Exit: the v0.1 feature list in [SyntaxMesh §72](syntaxmesh-todo-extensible-opensource-v5.md) works on representative Rust/TypeScript/JavaScript/Python/Bash repositories and Markdown/text documentation, with correctness, restart, incremental-work, query-latency, and extension-conformance evidence.

## 6. Required verification fixtures

| Fixture | Failure it must detect |
| --- | --- |
| Same source, new line numbers | Stable IDs change unexpectedly. |
| Rename/move and changed extractor version | Identity continuity or invalidation silently misbehaves. |
| Parse error/unresolved external reference | Unsupported knowledge is reported as certain. |
| File edit during an index run | Late results overwrite a later generation. |
| Crash between current projection and history publication | A generation is visible without its history record, or retry duplicates a generation. |
| Historical replay after edits/deletions | Reconstructed prior state differs from the graph root recorded for that generation. |
| Disable/enable verified history | Canonical facts, IDs, or query answers change. |
| Extension namespace collision/malformed event | Core state is corrupted or another extension's facts change. |
| Probe observation conflicts with static fact | Trust layers collapse into one claim. |
| Backend differential run | Turso and SQLite disagree on logical graph state. |
| Embedded vs daemon host | Transport creates different semantics. |

Benchmarks must report both correctness and cost: indexed files, parser work avoided, generation time, query latency, memory, storage, and context tokens. Use fixed fixtures, versions, and hardware notes. Do not set a performance claim without a measured baseline.

## 7. Settled foundation decisions and remaining acceptance

- Settled: MIT/Apache-2.0 dual licensing is declared in the workspace and both license texts are present.
- Implemented: public DTO/export records have explicit schema versions and typed IDs with round-trip fixtures. Long-term compatibility and release stability still require acceptance; encoding is no longer an unimplemented foundation decision. NDJSON remains export-only.
- Settled for this implementation: adapters consume crates.io `penelope = "0.1.0"` and `statechronicle = "0.1.0"`, resolved by `Cargo.lock`, not sibling-project path dependencies. Penelope owns durable full-engine publication; StateChronicle verification is opt-in under ADRs 0004 and 0023, with subsequent incremental verification work. Portable signing/proofs and guarantees against whole-store rewriting are not provided.
- Turso FTS capability probe: the pinned `turso` 0.8.0-pre.13 crate supports native FTS when its `fts` feature and experimental index-method builder flag are enabled. `fts_match(name, "scheduler")` matched the indexed name `Scheduler overview`, while `fts_match(name, "sched")` returned no match. The reproducible Rust-only probe is `cargo make turso-fts-probe` (`fixtures/turso-fts-probe`); it does not alter the production store dependency, schema, or query contract. This confirms FTS is token-oriented and cannot replace current arbitrary substring search. The shared InMemory/File/SQLite/Turso backend conformance fixture now also checks a mixed-case interior substring (`"IrS"` in `"first"`) against the canonical result. Remaining: decide whether a separate token-search API is useful, investigate exact substring candidate indexes, and assess feature size, transaction/update/delete behavior, migration/rebuild, and query-plan/performance before adopting any production index.
- Rust 1.98.1 is the initial toolchain and MSRV. Pull requests and pushes now run the complete contributor gate on Ubuntu, macOS, and Windows via `.github/workflows/ci.yml`; the local Linux run is verified, while the hosted macOS/Windows results remain to be observed before claiming those targets are green.
- Exact v0.1 language quality bar and benchmark repositories.

Current next-work distinction: foundation/layout/migration choices already
implemented are not reasons to create more architecture machinery. Open product
acceptance includes representative language/query quality, small-budget evidence
ordering, complete host/extension lifecycle guarantees and observed non-Linux CI.
ADRs 0314–0316 retain rejected optimization and diagnostic evidence; the release
context baseline does not close these wider gates. Reuse existing parsers,
workflow adapters, source-validation and evaluation harnesses for that work.

Record each choice as an ADR with evidence, alternatives, migration effect, and date. Avoid making these choices implicitly in implementation code.

Current upstream check (2026-09-27): Turso's [PRAGMA reference](https://github.com/tursodatabase/turso/blob/main/docs/sql-reference/pragmas.mdx) still labels MVCC experimental, and its [compatibility notes](https://github.com/tursodatabase/turso/blob/main/COMPAT.md) describe a durability caveat for overlapping statements on one MVCC connection. This supports the WAL-first/conformance-gated choice above. Pin a release and rerun the exact required workload before any mode change.

### Analytics export — Arrow feed and restartable Parquet partitions implemented

The v0.1 product source includes downstream DuckDB analytics.
[ADR-0098](adr/0098-typed-analytical-export-boundary.md)
sets the boundary before public schema work: in-process typed Rust/Arrow feed,
versioned deterministic Parquet artifacts, no reads of adapter-private SQL
tables, no synchronous OLTP/DuckDB dual write, and no NDJSON internal handoff.
The store portion of that feed now has `GraphStore::fact_history_page`,
specified in [ADR-0099](adr/0099-snapshot-pinned-fact-history-pages.md):
InMemory/File use the reference scan, while SQLite/Turso use separate first-page
and keyset-continuation SQL over their existing composite identity/sequence
index. Cursor pages are pinned to one retained generation, clip later
valid-until bounds to that snapshot, and need no migration. A shared fixture
checks bounded ordering, continuation after later publication, cross-backend
equivalence, and cursor binding; both durable query-plan tests confirm the
composite index. The `syntaxmesh-analytics` crate now implements a versioned
Arrow EAV schema for fact-history attributes and lazily maps bounded pages from
the store API; stable IDs remain 32-byte binary columns, and repeated
attributes carry deterministic ordinals. Its production dependencies are the
store port and Arrow crates; it adds no file I/O, NDJSON, transport, or service
coupling. The separate `syntaxmesh-analytics-parquet` adapter writes one
deterministic Parquet partition per page and advances a constant-size,
versioned JSON checkpoint only after partition and BLAKE3 sidecar publication.
It uses an OS file lock and same-directory temporary-file publication, so an
interrupted uncheckpointed page can be verified and adopted on retry. The
current fixture verifies Parquet schema metadata, orphan recovery, and
completed-export idempotence on InMemory. The shared conformance fixture now
also runs the exporter against InMemory, File, SQLite, and Turso and compares
the complete artifact file sets and bytes, including Parquet partitions,
checksums, and manifests. An explicit read-only verifier now audits every
partition, schema, checksum, directory entry, and aggregate row count without
opening a store; ordinary resume remains bounded to the latest checkpoint per
[ADR-0102](adr/0102-full-parquet-export-integrity-audit.md). Penelope-driven
scheduling is now available as an explicit workflow in the Penelope integration:
it journals the scoped export request before the idempotent Parquet effect and
returns its receipt after replay-validated completion ([ADR-0103](adr/0103-penelope-managed-analytical-export.md)). It remains downstream and opt-in; no daemon or
background runtime was added. The new `syntaxmesh-analytics-duckdb` crate
opens only complete exports after the full verifier succeeds, pins its
in-memory DuckDB view to that exact partition set, and exposes a bounded typed
query for distinct fact versions per valid-from generation ([ADR-0104](adr/0104-rust-duckdb-analytics-adapter.md)). [ADR-0105](adr/0105-analytical-generation-sequences.md)
carries the canonical generation sequence through store pages and Arrow/Parquet
schema v2, so DuckDB counts are chronologically ordered without sorting opaque
IDs. Analytics cannot participate in or delay graph publication. To support a
delta-proportional consumer,
`GraphStore::changes_between_page` now provides bounded, cursor-pinned generation
change pages; SQLite/Turso seek the existing generation-sequence primary key,
while reference stores use the conformance implementation. This additive store
port contract is recorded in [ADR-0106](adr/0106-bounded-generation-change-pages.md).
`GraphStore::fact_versions_changed_at` now supplies the typed fact versions
opened/closed by one accepted generation; SQLite/Turso use explicit
identity/start and identity/end indexes added through their registered
migration paths ([ADR-0107](adr/0107-generation-scoped-fact-version-changes.md)).
Cross-backend edit/delete fixtures cover the result, and both durable plan tests
assert the two indexed seeks. These feed contracts do not themselves implement
DuckDB synchronization or replace verified Parquet rebuilds. The new bounded
`fact_version_changes_at_page` store-port feed pages every version opened or
closed at a generation without first materializing its full `GraphDelta`;
SQLite/Turso use generation-leading start/end indexes added by their registered
v20→v21 and v23→v24 migrations ([ADR-0108](adr/0108-bounded-fact-version-change-pages.md)).
Cross-backend edit, file/provenance replacement, implicit edge-cascade, delete,
continuation, cursor-binding, and page-limit fixtures cover the feed. The
optional DuckDB adapter now has explicit schema bootstrap/validation, a
lock-pinned verified Parquet staging rebuild, typed Arrow-appender page writes,
and an atomic continuation-cursor/generation watermark. A partially applied
generation is hidden from the materialized query surface and resumes from its
persisted cursor after restart. The cross-generation update/delete fixture
compares the recovered materialization against a full verified Parquet query,
checks interval closure, and confirms a corrupt replacement export leaves the
last good materialization intact. The Penelope integration now journals explicit
sync/rebuild requests and recovers prepared requests after restart behind its
opt-in `analytics-duckdb` feature; Engine consumers do not pull DuckDB unless a
host enables that feature ([ADR-0109](adr/0109-duckdb-fact-history-materialization.md)). Injected transaction-failure coverage confirms row and cursor/watermark rollback together. Broader graph-metric/cross-snapshot queries remain open; the materializer remains optional and downstream.
