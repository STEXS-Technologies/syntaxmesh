# ADR-0290: Refine diagnostic seed priorities within bounded graph selection

Status: diagnostic refinement rejected for production routing.

Reuse the existing ranked selection report, global exact/stemmed scoring and
family composition. Start with ADR-0289's maximum 32 seeds; expand through the
existing ranked selector to maximum 64 nodes/two hops. Restrict the globally
scored channels to those selected IDs, then compose a new maximum 32 primary-first
seed plan. Scores must still use the complete generation, not neighborhood-local
rarity. This may replace original seeds; do not claim identical membership.

Keep navigation families eligible, retain original question and token budget,
and compile the refined plan with zero additional hops. This avoids an unbounded
second traversal and leaves the existing ranked compiler contract unchanged.
Do not inspect expected targets or paths when planning. Artifact policy names
must distinguish this experiment from ADR-0289. Require subset, capacity and
determinism checks before representative measurements; no default host changes.

Strict MCP Clippy and 22 non-ignored MCP unit tests pass. Initial selected-file
attempt exposed the neighborhood API's positive-hop requirement; the corrected
diagnostic reports the refined plan directly rather than invoking a zero-hop
neighborhood. A subsequent compile-only attempt exposed a depth-type mismatch;
both failed artifacts are retained and neither is quality evidence. Corrected
selected-file run `context-20261002T030955.284788Z-uncommitted` completes. Whole
repository evidence remains required before accepting this planner.

Whole-Sim `context-20261002T031036.487096Z-uncommitted` completes with the
unchanged default quality failure (3/10). Refined-plan source admission remains
4/5 at 8,192 tokens, but TypeScript regresses from selected at depth one in the
initial 64-node set to absent in the final 32-node plan. Python, JavaScript, Bash
and Quickstart still hit. The unchanged corpus publishes in 364.699 seconds;
identifier indexing takes 76.579712 seconds. Reject this 64-to-32 round-robin
refinement: it trades a packing failure for a plan-admission failure. Do not rerun
it unchanged on Shardline or promote it based on selected-file 5/5. Further work
must preserve the bounded selected evidence set while permitting explicit packing
priorities, rather than silently treating a 32-seed budget as its complete plan.
