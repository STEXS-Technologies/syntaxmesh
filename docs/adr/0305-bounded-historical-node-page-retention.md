# ADR-0305: Retain a bounded working set while scanning historical node pages

Status: implemented; representative performance validation pending.

ADR-0304 places 97.5% of the measured cold index build inside node-page reads.
The historical node reader currently discards all validated immutable pages
after every yielded node, forcing ancestor/successor pages to be fetched again.

Reuse Shardline memory-cache principles (bounded entry count plus charged bytes),
not its asynchronous loader framework. Keep the existing persistent-tree cache
and walker, scoped to one generation-pinned scan. Retain at most 128 pages and
4 MiB of charged page values plus fixed node metadata. Reset before an insertion
would exceed either limit. One individually oversized page may be held transiently
to advance the walker, then evicted on the next yield; this is not an RSS bound.

Continue validating every loaded page's content address, priority and payload.
Do not retain pages between store calls, bypass authoritative schema validation,
cache errors, alter cursor/ordering/lookahead behavior or change public contracts.
Verify entry/byte eviction, oversized handling, corruption and backend/restart
conformance. Representative timings are required before claiming improvement.

All 44 Turso unit tests, all 17 cross-backend scenarios (40.07 seconds) and
strict all-target/all-feature Turso Clippy pass. Selected-file indexed-host Sim
`context-20261002T111038.223333Z-uncommitted` passes all five required
8,192-token targets. Contributor CI is running; repository-scale performance
remains pending. The smoke predates a test-only error-conversion cleanup, not
a production-reader change.

Whole-Sim comparison artifact
`context-20261002T111147.442908Z-uncommitted` is running with the same
2,572-file fingerprint, instrumentation policy and disk-backed scratch root as
ADR-0304's profile. The input fingerprint is
`d085ad193619f9cb5957280fc2d47f93562a3ea6156c0baa69ecaf1710c91d24`.
Compare measured node-page regions and required-target outcomes after completion;
single local samples do not establish an SLA or isolate all host variation.

Full contributor CI completes successfully in 252.84 seconds with two jobs,
including all-feature workspace tests, all 17 backend conformance scenarios,
mixed instrumentation checks, strict Clippy, migration/extension gates and
documentation. Existing allowed dependency-policy warnings remain. Whole-Sim
performance evaluation is still live; CI does not establish a speedup.

The whole-Sim comparison subsequently completes successfully in 505.70 seconds.
Publication is 383.130 seconds; all five required 8,192-token targets still
pass, with 8/10 admissions across both budgets. The index counts are identical
to ADR-0304: 88 pages, 87,486 nodes, 567,068 postings and 21,071,384 charged
bytes. Build time is 38.428678 seconds (37.201757 seconds page reads,
1.226637 seconds processing), versus 42.733786/41.671530/1.061990 seconds in
the prior instrumented sample. Cold context takes 38.975621 seconds; retained
8,192-token requests take 0.881154–1.132319 seconds.

This single matching-input comparison shows a modest observed reduction, not
statistically established improvement or acceptable cold latency. Node-page
reads still dominate. Next evidence should separate authoritative schema/root
reads, page SQL lookups and loaded-page validation before choosing another
backend optimization; do not expand cache retention without measured need.
