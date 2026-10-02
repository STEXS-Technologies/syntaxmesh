# ADR-0312: Build the identifier index through the historical-node visitor

Status: implemented; contributor CI and whole-Sim fixed-target evaluation passed.

Use ADR-0310's object-safe `visit_historical_nodes` port for all identifier-index
build variants, including path and lexical-planner channels. This reuses the
existing reference paging behavior and ADR-0311's transaction-scoped Turso
implementation; do not add a second index, scan framework or schema cache.

Retain positive node/posting/payload budgets, increasing-ID validation, source
inventory checks and existing term/posting accounting. Build into private state
and return it only after the entire scan succeeds. Preserve the original Query
error from a failed insertion callback; never replace posting/payload errors
with a generic backend failure. Translate total node-budget exhaustion to the
existing identifier-index node-budget error. Other store errors remain errors.

For feature-gated benchmark metrics, add a separate logical `node_scans` count.
Keep `node_pages` for explicit page calls (zero in the visitor-based builder),
not hidden fallback paging or physical tree-page loading. Retain the historical
`page_read_elapsed` field name for compatibility, documenting it as store scan
time excluding measured callback processing. Record callback time separately;
derive scan time by subtracting it from the enclosing operation duration.
Backend instrumentation remains the authority for physical page-load counts.
Normal builds must not start clocks or emit benchmark logs.

Verify budget/error behavior, exact postings and planner/path parity, empty
generations, malformed ordering, both instrumented and normal builds, strict
Clippy and cross-backend/restart context conformance. Then rerun representative
whole-repository retrieval with the same corpus and scratch policy. A successful
integration or contributor CI alone does not establish speed or relevance.

The visitor-based builder is implemented without changing term/posting logic.
58 instrumented and 57 normal Query tests pass, including independent lexical
and planner oracle comparisons. Strict Query Clippy exposed an enum wildcard
in error translation; replace it with an explicit InvalidPageLimit predicate
and ordinary conversion for all other errors, without a lint allowance.
Final Clippy, host and cross-backend context checks are being completed.

Strict Query Clippy passes after that correction. All 27 regular MCP tests
pass (three manual evaluations ignored), and all three backend context scenarios
pass in 42.54 seconds. An additional restricted-store regression passes:
plain/planner builds succeed without page/snapshot ports and preserve the exact
payload-budget callback error. Full contributor CI and matching-policy whole-Sim
retrieval are running; their terminal results and latency acceptance remain open.

The first benchmark attempt stops before indexing because the earlier private
scratch parent no longer exists; the failed raw artifact is retained as
`context-20261002T125244.442003Z-uncommitted`. A new private scratch parent under
the same target filesystem is created with mktemp, and the same whole-Sim
workload/policy is rerun. No retrieval measurement came from the failed attempt.

Full contributor CI completes successfully in 198.28 seconds with two build
jobs, including all 17 backend scenarios (44.16 seconds), strict workspace
Clippy, mixed feature checks, architecture/extension/migration/dependency gates
and documentation. Whole-Sim evaluation remains live after preparing 2,572
files with unchanged input fingerprint `d085ad193619f9cb5957280fc2d47f93562a3ea6156c0baa69ecaf1710c91d24`.
Correctness is verified; representative latency and relevance results are pending.

Whole-Sim `context-20261002T125358.531726Z-uncommitted` completes in 526.30
seconds. Its unchanged input fingerprint, 87,486 nodes, 567,068 postings and
21,071,384 charged bytes match ADR-0307. One visitor scan replaces 88 explicit
node-page calls and reads the 33,580,768-byte authoritative history BLOB once
instead of 88 times. This is not one physical page: 90,556 pages are loaded.
Index construction takes 13.322831 seconds (11.825925 store work, 1.496277
callback processing); cold context takes 13.819686 seconds versus ADR-0307's
37.411926 seconds. All five required 8,192-token targets remain, with 3/5 at
2,048 tokens; retained 8,192-token queries take 0.723403–0.989992 seconds.

This is one matching-input debug comparison, not statistical speedup evidence,
an O(1) historical-query claim, general relevance proof or default promotion.
Cold usability remains open. Backend SQL page loading now dominates at
8.527266 seconds; any next optimization should reuse existing bounded batched
page-reader patterns and preserve snapshot validation, rather than introduce
another cross-generation cache or weaken authoritative checks.

Follow-up inspection narrows that candidate: PersistentFactTreeRangeWalker
discovers child IDs from loaded parent pages and reports one missing page at a
time; incidence traversal has the same dependency. Existing SQL batch readers
only batch already-known IDs. Blindly prefetching descendants would require a
new bounded frontier policy and explicit treatment of unvisited corruption and
budget stops, not merely copying a prepared IN query. Also, the measured SQL-page
region wraps read_many_prepared, which includes row payload transfer and
bincode deserialization into a Vec, not database execution alone. Separate these
costs using existing private profiling conventions before selecting the next
optimization. No prefetch implementation or storage change is authorized by this
inspection, and no further speedup is claimed.
