# ADR-0324: Compose explicit role preference in the indexed MCP host

Status: host implementation full CI passes; superseded fixture passes; corpus gate open.

Full contributor CI passes in 149.74 seconds for the response-level and all-role
explicit-seed parity coverage. Subsequently added superseded-role testing passes:
move intent between stable callables, publish, close/reopen, then alternate latest,
previous and latest roots. Each response retains both definitions, generation and
token bounds with its own role order; cache identity cannot leak the latest role
into the previous snapshot. Full CI for this additional fixture is not yet rerun.

All 28 active MCP unit tests pass (three manual corpus evaluations ignored),
including explicit builder/clone/cache/reset coverage; strict all-target MCP
Clippy passes. This does not yet prove role-sensitive current/historical host
output or corpus quality. Those evaluations remain required.

A response-level fixture now passes through both current and generation-selected
MCP routes: positive Tokio test intent sorts before/after the unclassified
callable as explicitly requested, and both definitions are retained within the
token budget. This uses the same generation in both routes, not a superseded
snapshot; role-sensitive superseded-history coverage remains open. Explicit-seed
parity coverage now includes all three preferences; full CI is running.

Reuse Query's generation-pinned role packing API rather than duplicate role
decoding or ranking in the host. An explicit builder enables indexed context
with a selected preference. The existing indexed builder resets preference to
neutral; ordinary hosts and explicitly seeded requests keep their existing path.
Clones retain preference and share the existing bounded planner cache.

No wire DTO, database, workflow, source execution or automatic query-intent
inference changes. Unknown metadata remains unknown. Corpus evaluation must
record the preference and cover test-oriented queries before default promotion.

The existing evaluation command accepts `SYNTAXMESH_CONTEXT_ROLE_PREFERENCE`
as `neutral` (default), `test-first` or `test-last`. Non-neutral preferences
require the indexed-host probe. Xtask validates this before starting evaluation
and captures the preference in bundle metadata; the direct MCP evaluation also
validates it and logs the selected policy. Existing required case/input gates
are unchanged. These settings are evaluation controls, not production wire fields.

Focused Xtask validation covers every accepted spelling, invalid/empty values,
and non-neutral routing requirements; it passes with strict all-target
Xtask/MCP Clippy. A fresh whole-repository Shardline release neutral baseline
is running against the clean owned d91b58b65b6f2e468d1ad53a9603d4fb94645dc7
corpus. The completed matching-source release pair rejects global test-last
default promotion. Neutral retains 6/6 large-budget and 4/6 small-budget targets;
test-last retains 5/6 at each budget and fails the required migration target at
both budgets. Webhook and benchmark documentation improve at the smaller budget,
but do not compensate for the required large-budget regression. Preserve neutral.

Both runs use 808 files and input fingerprint
`4419eb47c3925327b409c61efc391c8886aca746badb7fe664e4daae8c143795`;
source fingerprint is `b1ba7e095cd080942f1cf6ee07e02f8e4035d734535c68b96172ccfd9abe738c`.
Local evidence bundles:
`target/context-retrieval-results/context-20261002T153854.635355Z-29fdd51d2e88`
and `target/context-retrieval-results/context-20261002T154115.360269Z-29fdd51d2e88`.
The latter exits unsuccessfully with raw evidence retained. These single samples
do not establish a repeatable performance advantage. Test-oriented corpus cases
and a relevance-preserving alternative remain open. The target implementation
has no direct test attribute; therefore the regression is not evidence of that
function being classified as test intent. Investigate packing displacement.

Correction under verification: the raw large-budget miss includes the actual
implementation span at helpers.rs:1236–1282. The evaluator resolves its target
with `name.contains`, allowing the Function named
`apply_pending_local_migrations_is_idempotent` to stand in for the implementation.
Function target selection now requires exact name equality; document section
matching retains its existing phrase behavior. A focused regression checks this
collision. The previous pair's success/failure describes its ambiguous target
selection, not a proven implementation retrieval regression. Repeat both policies
with exact function identity before drawing quality conclusions; preserve neutral
in the meantime. This strengthens target identity rather than accepting snippets
without the intended node ID.

The corrected release pair completes successfully: exact implementation targets
retain 6/6 large-budget matches for both policies; neutral has 4/6 small-budget
matches and test-last 5/6. The gain is benchmark documentation; the actual webhook
implementation remains absent at the small budget. The earlier apparent webhook
gain and migration loss were ambiguous target-selection artifacts, not proven
implementation outcomes. Both new runs match source fingerprint
`976bf7c77f653c147d3d35a0fe024b7e636aa3487c87f128ff106cc459f7f180`
and the same 808-file corpus fingerprint above. Corrected local bundles:
`target/context-retrieval-results/context-20261002T154612.105840Z-29fdd51d2e88`
and `target/context-retrieval-results/context-20261002T154806.614337Z-29fdd51d2e88`.
Keep neutral default: explicit test-oriented corpus evaluation, broader repository
quality and repeated performance evidence remain required. Strict MCP Clippy and
the exact-name collision regression pass.

Full contributor CI now passes in 134.59 seconds for the complete host, history,
evaluation controls and exact-function-target correction, including all 17
backend/restart scenarios (41.21 seconds). Subsequently the Shardline corpus
matrix adds the migration idempotence test with its exact Function identity and
a natural-language test query. All six previous cases remain; Xtask requires
seven complete cases rather than accepting the old subset. This expanded matrix
still requires release evaluation under neutral and explicit preferences.

Initial seven-case setup stops before test-query evaluation: Rust's canonical
function name includes its containing `tests::` module. The case now specifies
`tests::apply_pending_local_migrations_is_idempotent` exactly. This is a fixture
identity correction, not evidence of retrieval failure or a relaxed matcher.

The corrected seven-case release matrix completes the explicit test-query check.
Neutral retrieves 7/7 large-budget and 5/7 small-budget targets; test-first
retrieves 7/7 and 2/7; test-last retrieves 6/7 and 5/7 and exits unsuccessfully
because the explicitly requested idempotence test is absent at both budgets.
The test is present under neutral at both budgets. Global role-first/last
preferences are therefore not general-purpose defaults. Keep neutral and expose
role preference only as an explicit host composition choice; do not infer it
from query words or treat absent metadata as production evidence.

All three runs match source fingerprint
`ba76a5d3842386afb426803846905b63743963b41551af0a7a304fc82bfb3c83`
and raw corpus fingerprint
`4419eb47c3925327b409c61efc391c8886aca746badb7fe664e4daae8c143795`.
Local bundles are `context-20261002T155453.396982Z-29fdd51d2e88` (neutral),
`context-20261002T155647.321336Z-29fdd51d2e88` (test-first), and
`context-20261002T155845.080749Z-29fdd51d2e88` (test-last), under
`target/context-retrieval-results/`. Failed-run metadata retains null input
fingerprint because final successful-run aggregation was not reached; its raw
stderr records the matching corpus fingerprint. Preserve that distinction.
Broader corpus quality and repeated performance evidence remain open.

Final full contributor CI passes in 135.08 seconds with the seven-case matrix
and qualified test identity, including all 17 backend/restart scenarios
(41.89 seconds), strict workspace checks and generated documentation. Manual
corpus evidence above remains separate from ordinary CI; failing test-last
quality is retained rather than hidden by CI success.

The broader neutral Sim check preserves the prior 2,572-file corpus fingerprint
but stops at exact target setup: the Python method's canonical name is
`SimStudioClient::execute_with_retry`. The fixture now names it exactly rather
than reintroducing substring selection. The TypeScript case passed at both
budgets before setup stopped; remaining Sim retrieval is not yet verified.
