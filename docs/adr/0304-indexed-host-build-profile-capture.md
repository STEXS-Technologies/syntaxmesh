# ADR-0304: Capture build metrics from the actual indexed-host cache

Status: implemented; selected-file capture verified.

Forward Query's existing benchmark instrumentation through an additive MCP
feature of the same name. Indexed-host benchmark runs enable that feature in
both prebuild and evaluation commands and record the policy in metadata.

After the first actual host request, the test reads metrics from the completed
cached index and emits one labeled raw benchmark line. Do not build another
index, introduce oracle work, log in production or change context DTOs. Existing
target checks, token budgets, hops and candidate bounds stay unchanged. Normal
host builds do not enable instrumentation by default.

Profiled timings must be labeled as instrumented. Single-file-region timings
are attribution evidence, not proof of an optimization or a CPU-time profile.

All 27 instrumented MCP tests, 16 xtask tests and strict MCP/xtask Clippy pass.
Selected-file Sim `context-20261002T105607.741128Z-uncommitted` passes all five
required 8,192-token targets. It captures one build line from the actual cache:
one page, 545 nodes, 5,027 charged postings, 185,782 logical payload bytes;
98,815 us total, 90,381 us node-page reads and 8,220 us processing. Metadata
records instrumentation and the evaluation command enables its feature. These
small-fixture regions do not establish repository-scale bottlenecks. Full
contributor CI and representative profiling remain in progress.

Whole-Sim instrumented artifact
`context-20261002T105711.619095Z-uncommitted` is running on 2,572 files with
fingerprint `d085ad193619f9cb5957280fc2d47f93562a3ea6156c0baa69ecaf1710c91d24`.
This matches the earlier Sim input, but instrumentation and disk-backed scratch
storage differ; attribution should use measured regions within this run, not
claim a controlled storage-independent speedup against the earlier run.

Full contributor CI for ADRs 0303–0304 passes in 209.29 seconds with two build
jobs, including all-feature workspace tests, all 17 cross-backend scenarios,
mixed instrumentation checks, strict Clippy, migration/extension gates and
documentation. Existing allowed dependency-policy warnings remain. Whole-Sim
profiling is still live; this CI result does not establish its bottleneck.

Whole-Sim profiling subsequently completes successfully in 500.65 seconds.
Publication takes 368.362 seconds. The actual cached index reports 88 node
pages, 87,486 nodes, 567,068 charged postings and 21,071,384 logical payload
bytes: 42.733786 seconds total, 41.671530 seconds page reads and 1.061990
seconds processing. About 97.5% of index-build wall time is within store page
reads; these include backend validation/hydration, not just physical I/O.
The first host request takes 43.233080 seconds; retained 8,192-token requests
take 0.713450–0.964509 seconds. All five required targets pass at 8,192 tokens;
8/10 pass across both budgets, with Python and Quickstart missing at 2,048.

This identifies the store node-page region as the next profiling/optimization
target on this corpus. It does not attribute the earlier timing difference to
statement reuse alone, establish a hardware-independent SLA or accept cold
latency/default routing. Reuse existing bounded page-loading patterns when
investigating backend validation, repeated page loads and SQL-call overhead.
