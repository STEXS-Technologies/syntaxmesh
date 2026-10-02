# ADR-0303: Profile cold identifier-index construction with existing feature gating

Status: implemented; representative profiling integration pending.

Reuse Query's `benchmark-instrumentation` convention for optional index-build
metrics. Expose a feature-gated immutable metrics value on a completed index:
node pages, nodes, charged postings/payload bytes, total elapsed time, cumulative
node-page read time and cumulative per-page processing time. Measure inside the
existing one-generation build without changing paging, budgets or ordering.

Total time includes manifest access and validation; processing time includes
page validation and term/posting construction. Store page time includes backend
validation and hydration. These are wall-clock regions, not CPU time or a
database-only attribution. Path inventory loading before this builder is not
included. Failed builds publish neither an index nor partial metrics.

Normal builds retain neither metrics nor clocks. Keep database/host dependencies
out of Query; add no profiler framework, storage format or production logging.
Verify counts and region relationships without asserting machine-specific timing.
Representative instrumented runs are needed before selecting an optimization.

All 58 instrumented Query tests and 57 normal Query tests pass, along with
strict all-target/all-feature Query Clippy and workspace formatting. The added
test checks populated/empty builds, exact charged counts, elapsed-region
relationships and budget-failure rejection without fixed latency assertions.
Host benchmark capture, representative profiling and full contributor CI for
this instrumentation remain pending.

Subsequent contributor CI including ADR-0304 host capture passes in 209.29
seconds; ADR-0304 records the scope. Representative whole-Sim profiling remains
live and no measured repository-scale bottleneck is yet established.
