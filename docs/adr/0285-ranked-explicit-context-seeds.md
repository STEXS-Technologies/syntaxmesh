# ADR-0285: Preserve an explicit ranked seed plan during context packing

Status: accepted for additive opt-in Query composition; implementation present,
broader verification pending. No production host ranking policy is accepted.

## Evidence

Selected-file Sim `context-20261002T012126.493351Z-uncommitted` composes
globally weighted exact/stemmed family channels into 32 ordered distinct seeds.
The TypeScript target `generateBlockDoc` is the seventh seed and is present in
the 64-node context selection at depth zero. Nevertheless its source evidence
is absent from the final 8,190-token pack under the 8,192-token limit; 46 source
items are omitted. Other source spans, including README chunks and the larger
`generateMarkdownForBlock` body, consume the admitted budget. Source admission
for the five selected-file cases is 4/5, not acceptance of this seed policy.

`context::select_nodes` assigns every explicit seed `u32::MAX` relevance and
stores them in a canonical-ID map. `candidate_order` compares item kind,
relevance, distance, granularity and finally stable key. The existing explicit
request therefore expresses membership, not caller priority. Preserving order
in the diagnostic selector cannot preserve packing order through that boundary.
This is a packing-priority diagnosis, not missing extraction or graph resolution.

## Proposed direction

Reuse the existing selection, bounded graph expansion, canonical hydration,
source verification, omission accounting and exact serialized token packer.
Do not add a second context compiler, new runtime, guessed edge or storage layer.

Add a separately named Query composition boundary for an ordered seed plan,
for both current and retained generations. Earlier distinct seeds have higher
priority; repeated IDs retain their first position and consume no distinct
slot. Validate the existing raw 32-input bound before deduplication. Keep all
existing request, generation, query, hop, candidate, source and token bounds.
Keep the original question as metadata and perform no additional lexical lookup.
The ordinary explicit-seed methods remain unordered and retain their behavior.

Assign distinct seed priority `u32::MAX - first-distinct-position`. Neighbors
retain zero relevance, exactly as in existing expansion; priority does not
propagate or change canonical-order traversal admission. Existing item-kind
precedence remains source, signature, path, summary; within each kind caller
seed priority precedes distance, granularity and canonical-key tie breaks.
Graph-path endpoint priority retains the existing saturating-sum behavior;
it does not promise ordered seed paths. Do not silently
reinterpret a seed vector in the existing DTO or advertise the new boundary
over MCP/HTTP until a separate host-policy decision is accepted. Rank is caller
priority, not a probability or proof that a candidate answers the question.

ADR-0286 subsequently supersedes canonical frontier admission for ranked methods:
frontiers honor existing seed priority with canonical ties. Neighbor relevance
and ordinary-method traversal remain unchanged.

## Acceptance gates

Whole-Sim `context-20261002T012914.538931Z-uncommitted` subsequently terminates
with ten default rows and the expected required-target quality failure (3/10).
Ranked-family explicit source admission is 3/5: JavaScript/Bash/Quickstart hit,
TypeScript is absent from selection, and Python is selected at depth zero but
not packed. All selections reach 64 nodes. Ordered compilation fixes the
selected-file packing diagnosis but does not establish whole-repository relevance
or accept the equal-channel allocation. Keep the additive compiler API opt-in;
do not promote this diagnostic policy or rerun it as though acceptance were pending.

Full contributor CI for the implementation, constrained-budget tests, retained
backend fixture and diagnostic integration subsequently passes in 182.77 seconds,
including all 17 backend scenarios and contributor gates. Whole-Sim ranked-family
run `context-20261002T012914.538931Z-uncommitted` is still live. Repository-quality
acceptance and production/default host routing remain open.

Only the dedicated family diagnostic now routes through ranked selection and
packing; older probes retain ordinary unordered semantics. Selected-file Sim
`context-20261002T012757.379648Z-uncommitted` admits all five targets at 8,192
tokens (TypeScript 8,149), compared with the prior family-plan 4/5 unordered
result. No budgets, graph edges or target-aware quotas changed. A subsequent
logging-only update names the packing priority explicitly. Strict MCP Clippy
passes; full CI and whole-repository evidence are still pending.

The shared Query fixture now searches constrained serialized-byte budgets and
requires a budget where reversing the plan admits only the corresponding first
seed's complete source evidence, with bounded token counts and nonzero omissions.
It also rejects unknown seeds and distinct plans exceeding candidate capacity.
All 46 Query tests and strict Query Clippy pass. The existing temporal backend
fixture adds a two-seed ranked baseline, checks current/historical equality on
InMemory/File/SQLite/Turso, then compares the retained pack after edits and durable
reopen. That backend scenario and combined Query/Turso strict all-target Clippy
pass. Full CI, diagnostic routing and repository source admission remain open;
the backend check uses a generous budget, not constrained-budget parity evidence.

Initial implementation adds four separately named ranked-seeded selection/packing
methods for current and historical generations in a focused Query sibling module,
delegating to the same compiler and graph expansion. The existing shared context
fixture verifies ordered source items, duplicate first-position priority, ordinary
unordered invariance under reversal, current/historical equivalence, preserved
question, exact serialized count, zero-relevance neighbors, empty seeds and the
raw 33-input rejection. All 46 Query tests and strict all-target Query Clippy pass.
Constrained-budget admission, unknown/over-capacity ranked requests, durable
cross-backend retention, diagnostic routing, repository evidence and full CI are
still required; these initial checks do not prove general retrieval improvement.

- Accept the public contract before implementation; keep pure Query boundaries.
- Dedicated multi-seed fixtures must distinguish priority from canonical-ID
  order and show constrained-budget source admission changing with plan order.
- Verify duplicate first-position semantics, empty/missing seeds, all existing
  budgets, deterministic ties, neighbor propagation and exact token accounting.
- Verify unchanged ordinary/unordered compilation and current/historical
  equivalence, then retained generations after edits and durable restart across
  InMemory, File, SQLite and Turso.
- Rerun selected-file source admission and whole Sim/Shardline separately;
  neither a synthetic fixture nor improved selected-file output proves general
  relevance. Preserve every failure and documentation case.
- Require strict Clippy, architecture checks and full contributor CI before
  promoting a production routing policy.
