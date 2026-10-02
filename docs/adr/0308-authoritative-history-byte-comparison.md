# ADR-0308: Validate a bounded history cache with SQL byte equality

Status: rejected after representative comparison; optimization removed.

ADR-0307 measures 88 transfers of the same 33.6 MB history record during one
index build. Reuse Shardline's bounded entry/byte cache principle rather than
adding a loader framework or changing immutable generation contracts.

Retain at most one successfully validated history payload, capped at 64 MiB.
Before reusing its schema, ask the existing database connection whether the
authoritative row is a BLOB exactly equal to those bytes. Return a scalar rather
than transferring the whole result payload. Binding still copies the cached
bytes and SQL still reads/compares the authoritative payload; this is not O(1),
an RSS bound or a physical-I/O reduction claim. Larger records keep the existing
digest/decode cache path without retaining bytes.

Changed payloads follow the existing full-read/hash/decode path. Missing rows
remain stale-base errors, generation switches replace the single cache entry,
and invalid rows never publish cached schemas. Keep direct SQL corruption
detection, restart behavior, store ports and storage formats unchanged. No new
dependency, migration or generation-only trust. Measure representative latency
before accepting this optimization.

All 48 instrumented Turso unit tests and all 17 cross-backend conformance
scenarios (44.27 seconds) pass. Added tests cover generation/size bounds,
same-length byte changes, SQL type changes and deletion of a warmed row; the
existing retained-history corruption regression still passes. Strict all-target/
all-feature Turso Clippy and formatting pass. Full contributor CI and whole-Sim
comparison `context-20261002T115133.044411Z-uncommitted` are running. Its
2,572-file input fingerprint matches ADR-0307; no measured speedup is claimed.

Full contributor CI subsequently completes successfully in 280.79 seconds with
two build jobs, including all 17 backend scenarios (42.87 seconds), all-feature
workspace tests, strict workspace Clippy, mixed instrumentation checks, migration
and extension gates, dependency policy and documentation. Existing allowed
dependency-policy warnings remain. Whole-Sim comparison is still live; CI proves
the covered correctness gates, not latency improvement or default promotion.

Whole-Sim comparison subsequently completes successfully in 558.07 seconds.
All five required 8,192-token targets pass with identical index counts. Build
time is 40.368097 seconds (39.196957 seconds page reads), and cold context
takes 40.948480 seconds, versus ADR-0307's 36.901669/37.411926 seconds.
The 87 equality checks compare 2,921,526,816 cumulative bytes and take
24.711126 seconds. Binding/comparing the large payload does not remove the
dominant work and retains an additional 33.6 MB for this corpus.

Reject and remove the exact-byte retention/equality path rather than keeping
unproven overhead. Restore the ADR-0307 digest-only cache and profiling;
retain the new same-length/type/missing-row validation test in adapted form.
This one sample does not establish a statistical regression, but supplies no
evidence to justify the additional cache. The next solution must avoid repeated
full-record processing, not merely move it into SQL. Post-removal checks pending.

Post-removal, all 47 instrumented Turso unit tests and strict all-target/
all-feature Turso Clippy pass. Formatting and patch whitespace checks pass.
Full contributor CI is running after removal; the prior successful CI result
applies to the experiment, not the restored final worktree.

Post-removal full contributor CI subsequently completes successfully in 229.41
seconds with two jobs, including all 17 backend scenarios (43.87 seconds),
all-feature workspace tests, strict Clippy, mixed instrumentation, extension/
migration checks, dependency policy and documentation. This verifies the
restored digest-only implementation plus the retained regression test. Existing
allowed dependency-policy warnings remain; the full-record transfer bottleneck
is unresolved.
