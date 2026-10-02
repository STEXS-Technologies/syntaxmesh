# ADR-0289: Diagnose primary evidence priority without dropping navigation seeds

Status: diagnostic only; not accepted for production routing.

Whole-Sim evidence after ADR-0288 shows costly navigation occurrences preceding
Python's selected method in the ranked seed plan. Reuse ADR-0284's exact/stemmed
global statistics, bounded round-robin seed membership, canonical family classifier
and ADR-0285's ranked compiler. After selecting the same maximum 32 seeds, stably
partition code/documentation before reference/other seeds. Preserve order inside
both groups and retain every selected identity. No target paths, language quotas,
model calls, extra bounds or external reranking integration are introduced.

Identify the revised diagnostic policy explicitly in artifacts. Older equal-family
results remain rejected, not overwritten. This experiment does not prioritize
graph-discovered definitions automatically and may still miss TypeScript packing.
Require membership/order invariants and selected/whole-repository measurements
before any production decision. A passing compiler test is not policy acceptance.

All 22 non-ignored MCP unit tests and strict all-target MCP Clippy pass. The
stable-partition invariant covers all 256 primary-membership assignments of eight
seeds, including idempotence and exact relative order. Selected-file benchmark
`context-20261002T022638.121003Z-uncommitted` completes successfully. Its scope is
not whole-repository quality acceptance; representative verification remains open.

Whole-Sim `context-20261002T022720.997895Z-uncommitted` finishes with the
required-target default quality failure (default unchanged at 3/10). The same
2,572-file fingerprint publishes in 367.019 seconds; identifier indexing takes
75.295056 seconds. Primary-first ranked-family source admission improves from
3/5 to 4/5 at the unchanged 8,192-token budget: Python now hits, alongside
JavaScript, Bash and Quickstart. TypeScript remains selected at depth one but
misses packing. Seed membership and bounds are unchanged. This is evidence for
priority separation, not production acceptance or a full retrieval solution;
graph-discovered evidence admission and Shardline regression verification remain
open. Do not count these diagnostic hits as default host improvements.

Whole-Shardline regression run `context-20261002T023822.268791Z-uncommitted`
is live. Its prepared 810-file fingerprint is
`6c1ca43f67f1933ec6799c776043464d018e189f3f8593e63a8b589310b57872`,
identical to the earlier whole-Shardline baseline. The implementation fingerprint
matches the completed primary-first Sim run. Do not claim Rust regression safety
until terminal source-admission results are inspected.

Read-only compiler review while regression measurement runs confirms the remaining
TypeScript boundary: RankedExplicit assigns explicit seeds descending near-maximum
priorities, while expand_neighborhood assigns admitted neighbors relevance zero.
Source item ordering uses relevance before distance/granularity. Thus selecting
the target at depth one does not give it admission priority over seeded source
excerpts. This is documented ranked-compiler behavior, not an absent Calls edge
or stale-source failure. Any graph-derived priority policy requires a separately
recorded contract decision and invariant coverage; do not silently alter existing
ranked semantics or inflate traversal/token limits to mask the gap.

Whole-Shardline regression run is now terminal with the unchanged default quality
failure (Python metadata missing; default 6/10). Primary-first ranked-family source
admission is 5/5 at 8,192 tokens: Rust webhook, Python metadata, Bash query and both
documentation targets hit. Publication takes 933.904 seconds; identifier indexing
takes 265.390879 seconds. This fixture retains the Rust evidence lost by an earlier
hard definition filter and supports keeping navigation seeds. It does not establish
universal regression safety. Together with whole-Sim 4/5, this warrants further
graph-discovered admission work, not production promotion or an identical rerun.
