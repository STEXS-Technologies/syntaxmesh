# ADR 0274: Bounded historical node batches

Status: accepted, 2026-10-01

Whole-Sim diagnostic `context-20261001T153723.906030Z-uncommitted`
isolates repeated node hydration: manifest reads are 26–30 microseconds,
while 34–256 separate node reads take 0.230–2.768 seconds, approximately the
entire indexed-query duration. All candidates still equal the reference.

Copy Shardline's parameterized batch-loader pattern into the existing store
port. `historical_nodes_by_ids` accepts at most 256 input IDs, deduplicates and
returns existing nodes in stable ID order for one retained generation. Missing
IDs are omitted, an empty request still validates generation existence, and
oversized requests fail before reads. Reference backends reuse point reads;
Turso resolves sequence once then executes one parameterized IN query against
existing fact-version indexes with identical interval/latest-version semantics.
No migration, new database dependency or canonical search change.

The identifier channel hydrates its bounded selected IDs through this method,
validates returned membership/uniqueness and rejects missing candidates. Ranking
order is restored from the score list, not backend row order. Conformance must
compare point reads across retained versions, deletions and durable restart.
Post-change real-corpus cost measurement remains required; this decision is
not a retrieval-quality improvement or an O(1) guarantee.

The IN-query experiment is rejected by whole-Sim bundle
`context-20261001T154246.540209Z-uncommitted`: all five results remain equal,
but indexed queries rise to 0.758–6.985 s versus 0.232–2.800 s before it.
Repeated point hydration remains 0.248–2.773 s in the same run. Do not keep
the slower SQL shape. Reuse the already successful prepared-statement principle:
resolve generation once, prepare the identical point-version query once, bind
each unique ID in order. The bounded port contract remains; it is not a promise
of one SQL execution. Verify the prepared variant before claiming a cost win.

Implemented in the store port and Turso adapter; the identifier channel now
uses it without altering scores. All 37 Query tests pass. Expanded retained
node-page conformance passes across InMemory/File/SQLite/Turso, including
reopen/cold copies, duplicate/missing IDs, empty requests and invalid limits
(11.50 seconds locally). Strict Clippy initially caught a reference collection
style issue; the explicit deduplicated set is now iterated by reference.

Prepared variant bundle `context-20261001T154711.669837Z-uncommitted` keeps
all five results equal. Indexed query seconds are TypeScript 2.681943, Python
2.064984, JavaScript 0.236422, Bash 1.092460, Quickstart 0.926454;
repeated-point hydration is 2.714559/2.089690/0.242411/1.113645/0.951381.
This removes the IN regression but does not establish a meaningful gain over
the original point path. Retain no performance claim. Inspect execution plans
before further SQL-shape tuning; natural-language acceptance still fails the
same three cases. Strict Turso Clippy and retained-node-page conformance pass
for the prepared implementation (11.39 s locally).

Execution-plan evidence identifies the bottleneck: the actual Turso point SQL
chooses `syntaxmesh_fact_versions_time_idx (fact_kind, valid_from_sequence)`
for the outer lookup, omitting fact identity from the seek, and the identity/end
index for its MAX subquery. Pin both clauses to the already registered
`syntaxmesh_fact_versions_identity_time_idx`, following existing indexed
fact-version-change queries. Share the SQL literal across point/batch/evidence
readers and assert both plan seeks include identity/time. No migration or fact
semantics change; rerun conformance and whole-corpus equivalence/timings.

Whole-Sim post-plan bundle `context-20261001T193353.113460Z-uncommitted`
preserves all five complete candidate/payload/score comparisons. Indexed query
microseconds are TypeScript 27,702, Python 26,912, JavaScript 6,972,
Bash 15,681, Quickstart 1,943, versus 236,422–2,681,943 microseconds
before the plan fix. Separate repeated-point hydration takes 2,257–16,285
microseconds in the new run. These are sequential local measurements with
uncontrolled cache effects, not an SLA or identical workload-stage timing;
the improvement supports identity-constrained seeks. Full-scan oracle queries
still take 18.42–18.74 seconds. The plan fixture confirms both lookups seek
fact kind, fact ID and sequence on the existing identity/time index; retained
node-page conformance and strict Turso Clippy pass. Natural-language quality
still fails TypeScript/Python/Bash and default ranking is unchanged.

The complete contributor gate after the plan/index/batch changes passes in
219.44 seconds: architecture boundaries, independent extension fixtures,
migration registries, locked all-feature builds, strict workspace Clippy,
dependency policy, workspace tests/doc-tests, all 16 backend-conformance
scenarios and API documentation. The worktree was unchanged during the run.
This integrated verification does not close the separate retrieval-quality gate.
