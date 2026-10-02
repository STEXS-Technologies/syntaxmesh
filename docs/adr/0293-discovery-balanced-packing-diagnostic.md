# ADR-0293: Balance lexical and graph-discovered evidence in diagnostic packing

Status: diagnostic only; whole-Sim and whole-Shardline 5/5; integration pending.

ADR-0292 retains every selected identity, but its globally scored family order
still omits a graph-discovered TypeScript caller body. Reuse the existing bounded
ranked-plan compiler and round-robin refill helper, not another retrieval engine.

Partition the complete ordered plan into original lexical seeds and discovered
IDs, preserve each partition's order, and alternate lexical then discovered IDs
until exhaustion. Stably prioritize code/documentation over navigation after
composition. Keep all selected IDs, including zero-score discoveries, the same
64-candidate/two-hop selection and 8,192-token packing limits. No target names,
paths, language quotas or second traversal may influence this ordering.

This is caller packing intent, not inferred confidence or new graph facts.
Production host routing remains unchanged. Require membership, partition-order,
empty-channel and deterministic tests, then selected-file and whole-repository
measurements before considering promotion.

All 23 non-ignored MCP tests and strict all-target Clippy pass. Exhaustive
eight-ID partitions verify membership, lexical/discovery relative order and
determinism; balanced and empty-channel cases are covered. Selected-file run
`context-20261002T034640.362077Z-uncommitted` completes in 14.29 seconds.
Its diagnostic source admission is 5/5 at 8,192 tokens. Whole-Sim run
`context-20261002T034707.306262Z-uncommitted` is now running.
Whole-repository evidence remains required; this does not change production
routing or close the quality gate.

Whole-Sim `context-20261002T034707.306262Z-uncommitted` completes with
diagnostic source admission 5/5 at 8,192 tokens (TypeScript 8,192, Python 8,144,
JavaScript 8,088, Bash 8,158 and Quickstart 8,137). It admits the previously
omitted TypeScript caller without raising bounds or changing membership.
Publication takes 362.992 seconds and identifier indexing 76.521950 seconds
for the unchanged 2,572-file corpus fingerprint. Default retrieval remains
3/10 and fails the required-target gate (nested exit 101, task exit 105), so
the overall benchmark is not a passing default-quality result. Representative
Shardline regression and production planner/index integration remain required;
this diagnostic does not establish generalization or latency acceptance.

Full `cargo make ci` passes in 130.72 seconds, including all 17 backend
conformance scenarios, workspace strict Clippy, architecture boundaries,
migration registries, independent extension import and documentation build.
Whole-Shardline regression `context-20261002T035749.640093Z-uncommitted`
is running on the unchanged 810-file corpus. Contributor CI does not replace
that retrieval-quality evidence or authorize production promotion.

Whole-Shardline `context-20261002T035749.640093Z-uncommitted` finishes with
diagnostic source admission 5/5 at 8,192 tokens: Rust webhook 8,129, Python
metadata 8,140, Bash query 8,135, benchmark documentation 8,080 and rationale
8,192. The unchanged 810-file corpus publishes in 935.195 seconds; identifier
indexing takes 248.521774 seconds. Rust evidence is retained while whole-Sim's
graph-discovered TypeScript gap is closed. Default retrieval remains 6/10 and
fails its required-target gate on Python metadata (nested exit 101, task exit
105). This supports implementing an opt-in indexed equivalent under ADR-0294,
not promoting the complete-corpus oracle or claiming general quality/latency.
