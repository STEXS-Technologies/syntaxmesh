# ADR-0316: Reuse explicit-target probes before changing retrieval

Status: diagnostic changes and full contributor CI verified; ranking improvement pending.

Sibling implementation review (2026-10-02): Shardline's Rust index adapters
provide deterministic repository filtering and database ordering, not a
content-relevance/context-budget reranker. Sim's
`apps/sim/tools/pinecone/search_text.ts` forwards optional rerank settings to
Pinecone; it does not implement local ranking. Neither is a suitable mechanism
to copy for this gate. Do not introduce a remote reranking service or another
runtime to address ordered admission pressure.

The current lexical planner scans each exact/stemmed posting set separately for
four families, although rarity statistics are generation-global. A reusable
single-channel scoring pass followed by family partitioning is a potential
bounded-work simplification, independent of relevance-policy changes. It must
partition the complete ranked channel before truncation: truncating globally
at 256 first can silently lose a family's candidates. Verify independent-oracle
parity, posting-budget semantics and existing public error behavior before
adopting this optimization. It alone will not fix small-budget target omissions.

Full contributor CI passes in 146.20 seconds, including all 17 backend/restart
scenarios (46.92 seconds), all 14 daemon lifecycle tests, strict workspace Clippy,
architecture/migration checks and documentation. The expanded diagnostic and
probe metadata are verified. The production small-budget omissions remain;
passing diagnostic-code CI is not retrieval-quality acceptance.

Inspecting the captured JSON confirms both missed targets have initial selection
depth zero: they are original lexical seeds, not solely graph discoveries.
Generic normalized lexical ranking places Python's implementation 222nd and
Quickstart third. Thus replacing the bounded family plan with a single global
label ranking is not supported, and seed-first ordering alone cannot be assumed
to solve both. Keep lexical channels and graph-derived evidence distinct when
testing policy alternatives. Quickstart ordering and Python implementation/test
relationships require separate characterization before choosing one general
mechanism. Full contributor CI remains live; no ranking change is made here.

Budget-labelled capture `context-20261002T141236.463870Z-uncommitted` reproduces
the indexed family plan's three-of-five small-budget retention and five-of-five
large-budget retention. Python's small pack contains two retry-test bodies plus
SDK documentation, but no target evidence/signature. Quickstart's small pack
contains `vitest.setup.ts:1-116`, `executor-mocks.ts:61-107` and a different
README heading at lines 82–83. Neither pack reports source-validation warnings.
Combined with target-seeded recovery, this identifies ordered admission pressure
as the immediate cause, not stale bytes or an intrinsically unfit target span.
The ordinary default-host acceptance gate still fails three required targets;
do not represent the diagnostic bundle as a passing whole-product evaluation.

Regular MCP tests and strict MCP Clippy pass after the two-budget extension.
Run full contributor CI for diagnostic and metadata changes. A future general
ordering policy must retain bounded work, canonical-oracle parity, exact token
accounting, provenance, current/historical consistency and larger-budget targets,
and be evaluated beyond these two known queries. No hardcoded corpus/file/query
exceptions or automatic promotion are authorized by this evidence.

The graph/family-plan probe now compiles the same selected IDs at both benchmark
budgets and labels pack, target-kind, warning and span diagnostics with budget.
Generation and exact token limits are checked for each pack. This reuses existing
planner/compiler/source checks without special query strings, forced targets or
production changes. Strict MCP Clippy passes; regular tests and a fresh clean
whole-Sim capture verify the new diagnostic before any policy proposal.

Family-plan capture `context-20261002T140926.818887Z-uncommitted` reports
Python's expected target at packing position 13 and Quickstart at position 7.
The diagnostic family-plan packs retain all five targets at 8,192 tokens,
matching indexed-host behavior; the unrelated ordinary default-host gate again
fails TypeScript/Python/Bash. This run is diagnostic evidence, not an accepted
retrieval evaluation. Expected targets are present in the composed plans, so
the next investigation is ordering/admission at 2,048 tokens, not forced seeds,
an assumed oversized target, or more database caching. Extend existing plan
diagnostics to small budgets before choosing a production ranking policy.

The clean rerun bundle `context-20261002T140719.562021Z-uncommitted` records
the target-seed probe enabled in metadata and reproduces all ten explicit-target
successes, including Python/Quickstart at 2,048 tokens. Its ordinary default-host
gate still fails for TypeScript, Python and Bash; preserve that failure rather
than weakening acceptance. Extend the existing family-plan diagnostic with the
expected target's packing rank to inspect real ordering against its canonical
oracle. All 27 regular MCP tests pass (three opt-in evaluations ignored).
A separate whole-Sim family-plan capture is running; production remains unchanged.

Extend the existing opt-in target-seed probe to 2,048 and 8,192 tokens, with
generation/token-bound checks unchanged and budget labels in its output.
Record `SYNTAXMESH_CONTEXT_TARGET_SEED_PROBE` in evaluator metadata and extend
the metadata regression test. No production ranking or packing contract changes.
Strict MCP/xtask Clippy, formatting and the metadata regression test pass.

Partial bundle `context-20261002T140424.101924Z-uncommitted` shows explicit
target evidence retained for all five Sim targets at both budgets, including
Python at 1,936 tokens and Quickstart at 2,031 tokens. Inputs match the existing
2,572-file fingerprint. This falsifies the hypothesis that these target facts
necessarily exceed the smaller budget. Python's unseeded pack favors retry tests;
the long inline README logo is not proof of the Quickstart target's omission.

The overall diagnostic run fails its ordinary default-host required-target gate
for TypeScript, Python and Bash. It is not a passing indexed-host evaluation:
target probes deliberately cannot be combined with indexed-host routing.
Moreover, source edits occurred during this run's prebuild, so its initial
metadata fingerprint/probe map does not fully describe the compiled diagnostic.
Retain the partial logs as clues, then run a clean capture with the final sources
and metadata. Do not use forced target seeds as normal retrieval or substitute
them for natural-language quality acceptance. Investigate the existing indexed
planner's eligible facts and packing order next.
