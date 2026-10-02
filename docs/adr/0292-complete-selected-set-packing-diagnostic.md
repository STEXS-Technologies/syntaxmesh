# ADR-0292: Preserve the complete bounded selected set during diagnostic packing

Status: diagnostic only; whole-Sim quality gap confirmed; not promoted.

Reuse ADR-0291's ranked packing-plan API. Keep the initial maximum-32 primary-first
seeds and existing ranked selection bound of 64 nodes/two hops. Compose ordered
global exact/stemmed family channels restricted to the selected IDs, now retaining
all selected identities rather than reducing the set to 32. Append any zero-score
selected IDs canonically, then stably prioritize code/documentation evidence.
Reject foreign selected IDs or sets beyond the 256-entry packing-plan bound.

Use zero-hop selection/packing with the unchanged 64-candidate/8,192-token bounds.
Do not infer graph facts from ordering, filter navigation families, use expected
targets or expand a second time. Scores still use the complete generation.
Artifact policy names distinguish this from rejected ADR-0290 refinement.
Require exact set equality, zero-score retention, determinism and capacity tests,
then representative quality evidence before any host routing decision.

All 22 non-ignored MCP tests and strict all-target MCP Clippy pass. The extended
fixture checks exact 64-ID retention under matching, unrelated and empty queries,
determinism, foreign-ID rejection and 256/257 capacity boundaries. Selected-file
run `context-20261002T033234.720067Z-uncommitted` admits all five source targets
at 8,192 tokens. Whole-repository quality remains required; production routing
is unchanged and selected-file success does not supersede earlier rejections.

Whole-Sim run `context-20261002T033322.188476Z-uncommitted` finishes with
diagnostic source admission 4/5 at 8,192 tokens: Python, JavaScript, Bash and
Quickstart hit, but TypeScript still misses packing. Default retrieval remains
3/10 and fails the required-target gate (nested test exit 101, task exit 105).
The unchanged 2,572-file corpus publishes in 359.372 seconds; identifier indexing
takes 75.494537 seconds. Complete selected-set retention does not suffice to
resolve evidence allocation. Do not promote this policy or repeat the unchanged
experiment on another corpus as if the TypeScript gap were closed. Inspect the
retained plan and packing priorities before the next implementation change.

Artifact inspection confirms the target is initially graph-discovered at depth
one, retained in the complete plan, and hydrated in zero-hop selection (reported
depth zero is plan membership, not its original graph distance). Packing reports
no source warnings and no target-attributed items. The first packed excerpt is
the related `generateMarkdownForBlock` helper at lines 959–1147 (6,135 rendered
bytes), not `generateBlockDoc` at lines 897–957. Do not mistake nearby same-file
content for target retrieval or weaken the node-attribution oracle. The selected
caller body is omitted under the existing budget; the next experiment should
address graph-discovered primary evidence priority without hard-coded target
names, paths or language-specific quotas.
