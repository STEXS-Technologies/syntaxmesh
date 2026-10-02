# ADR-0300: Measure the opt-in MCP host path independently of diagnostics

Status: accepted for benchmark integration; default routing unchanged.

Extend the existing retrieval evaluation with the explicit test-only environment
flag `SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE`. Enable ADR-0298's host builder for
that run, retaining the existing natural-language cases, required target-node
checks, 2,048/8,192-token budgets, one hop and 64 candidates. Do not substitute
diagnostic two-hop selection or target hints. Label host policy in raw evidence
and retain the flag in artifact provenance.

Reject combination with corpus/graph/identifier diagnostic probes so this run
measures the actual host without complete-corpus oracle scoring or a second
planner index. The first natural-language context request includes cold index
construction; subsequent requests share the host cache. Record that distinction
explicitly instead of presenting cold and warm work as equivalent samples.
Existing exact token accounting and required-target gates remain unchanged.

Require selected-file smoke evidence before whole-Sim and current-Shardline
host runs. Keep corpus fingerprints, versions and raw per-case query timings;
separate current-repository validation from same-input comparison when the
corpus changes. No default promotion or generalized latency/SLA claim follows
merely from diagnostic success or one sample.

Benchmark integration is implemented with a distinct policy marker, captured
flag, incompatible-probe rejection and per-request cold/retained cache state.
All 27 MCP tests and strict MCP/xtask Clippy pass. Selected-file Sim
`context-20261002T100153.512522Z-uncommitted` completes successfully in
8.80 seconds: all five required targets are admitted at 8,192 tokens, with
one hop and no diagnostic oracle. The first 2,048-token request includes cold
construction (705.182 ms); subsequent 8,192-token requests take
31.829–865.556 ms on this small fixture. The first TypeScript request does not
admit the target at 2,048 tokens; no small-budget completeness is claimed.
Whole-Sim host evaluation and contributor CI for benchmark integration are
running. Current-Shardline host validation remains pending; default mode is
unchanged.

Current-Shardline actual-host artifact
`context-20261002T101237.524395Z-uncommitted` terminates with execution failure
after 728.07 seconds: publication reports `upsert edge batch: I/O error
(pwritev): quota exceeded`. No context-quality or latency result was produced.
The prepared corpus has 842 files and fingerprint
`157bbd788278ffcc5cdfd0b2de87df3038afeec3dca86fe0a3346099f00cb672`;
it differs from the earlier 828-file diagnostic corpus. Available filesystem
space after termination does not establish the cause of the quota failure.
Current-Shardline actual-host validation remains open.

Subsequent disk-backed run `context-20261002T102712.399141Z-uncommitted`
completes successfully; ADR-0301 records its 844-file fingerprint and statement
reuse. All five required 8,192-token targets pass, with 8/10 admissions across
both budgets. Cold context takes 182.823160 seconds; retained 8,192-token
requests take 0.902398–1.062620 seconds. This closes the fixed-target
current-Shardline host check, not cold-query usability or default promotion.

Benchmark-integration contributor CI passes in 130.42 seconds with two jobs
after correcting the metadata test's old eight-flag expectation to nine and
explicitly testing indexed-host flag capture. All 15 xtask tests pass. The
running whole-Sim evaluator predates this test-only correction; its compiled
host path and captured policy are unchanged. Repository-scale host retrieval
results remain pending.

Whole-Sim actual-host run `context-20261002T100218.001743Z-uncommitted`
finishes successfully in 552.66 seconds on the unchanged 2,572-file fingerprint.
All five required targets are admitted at 8,192 tokens with one hop and 64
candidates; the ten budgeted requests admit eight targets total (Python and
Quickstart miss at 2,048). Publication is 387.895 seconds. The first context
request includes cold index construction and takes 78.732362 seconds; retained
8,192-token requests take 0.718761–0.974735 seconds, including canonical
selection, source validation and exact packing. This is one debug-mode local
sample, not an SLA or proof of cold-query usability. Current-Shardline actual
host evaluation is running. Default routing remains unchanged.
