# ADR-0306: Attribute historical node-read work inside the existing benchmark feature

Status: implemented; selected-file capture verified.

Extend existing Turso benchmark instrumentation with private scan-local timings
for authoritative schema reads, persistent-root reads, SQL page loading (including
row decoding), page integrity validation and node fact decoding. Count loaded
pages and decoded nodes. Successful scans emit a labeled raw line only when
`SYNTAXMESH_TURSO_NODE_READ_PROFILE` is enabled in an instrumented build.

Forward the existing Turso feature through MCP benchmark instrumentation. The
indexed-host evaluator enables the private profile in its child process and
records that policy in metadata. Keep store ports, production DTOs, storage
formats, paging and validation unchanged; no new dependency or profiler framework.
Region timings are wall-clock work, not physical-I/O or CPU attribution. SQL
prepare/walker overhead remains in total-minus-labeled regions.

All 46 instrumented Turso unit tests, 16 xtask tests, strict all-feature
Turso/MCP/xtask Clippy and formatting pass. Selected-file Sim artifact
`context-20261002T112443.582902Z-uncommitted` retains all five required targets
and records the private policy. Its 545-node scan loads 577 pages and takes
58,047 us: schema 9,306 us, root 595 us, SQL/row decoding 37,870 us, page
validation 3,517 us and fact decoding 5,383 us. The cached index takes 66,887 us
total. This verifies capture, not repository-scale attribution or improvement.
Full contributor CI and representative finer profiling remain pending.

Whole-Sim finer-profile artifact
`context-20261002T112605.725189Z-uncommitted` is running on the same 2,572-file
fingerprint `d085ad193619f9cb5957280fc2d47f93562a3ea6156c0baa69ecaf1710c91d24`
with disk-backed scratch storage. Metadata records the private profiling policy.
This is a finer-attribution run, not a storage-independent speedup experiment.

Full contributor CI completes successfully in 228.09 seconds with two build
jobs, including all-feature workspace tests, all 17 cross-backend scenarios,
mixed instrumentation checks, strict Clippy, migration/extension gates and
documentation. Existing allowed dependency-policy warnings remain. Whole-Sim
finer profiling is still live and its measured attribution remains pending.

The whole-Sim run subsequently completes successfully in 539.65 seconds.
All five required 8,192-token targets pass; three of five pass at 2,048 tokens.
The index contains 87,486 nodes and 567,068 postings across 88 pages, with
21,071,384 charged bytes. Index build takes 40.658077 seconds, including
39.352074 seconds in node-page reads and 1.305606 seconds in processing.
The first context request takes 41.239454 seconds; retained 8,192-token
requests take 0.829554–1.123132 seconds in this single debug sample.

Aggregating only the 88 scan lines between host policy and index-build output
attributes 26.403471 seconds to authoritative schema reads, 10.735801 seconds
to SQL page loading/row decoding, 1.079056 seconds to fact decoding,
0.746211 seconds to page validation and 0.087200 seconds to root reads.
Those scans load 92,490 pages and decode 87,573 nodes, including page lookahead.
Schema reads dominate this workload, unlike the selected-file smoke. The next
optimization should reduce repeated full history-payload loading while preserving
authoritative corruption detection; simply trusting a generation-only cache is
not acceptable. No speedup, cold-latency acceptance or default promotion follows
from this attribution run.
