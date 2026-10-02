# ADR-0329: Explicit lexical-first discovery packing

Status: Query and MCP composition verified; corpus quality gate remains open.

## Evidence and decision


Reuse Shardline's separation of explicit search options from default behavior
(`shardline-index/src/hub.rs`, `HubRepoSearchOptions`) and SyntaxMesh's existing
source-role packing composition. Add an explicit balanced/default versus
lexical-first preference. Lexical-first stably partitions the complete existing
role-ordered plan by original-seed membership. Within each partition, existing
ordering is preserved. Discovery preference is the outer partition, not a new
confidence claim or source-role classifier.

## Invariants and gates

No extraction, graph selection, membership, provenance, generation validation,
posting budget or token budget changes. Existing entry points remain unchanged.
Query remains independent of host/runtime dependencies. Test balanced parity,
stable partition, complete membership and role composition before host adoption.
Do not promote a default or claim corpus improvement without response-level
evaluation, including discovery-dependent queries and retained generations.

## Verification

All 62 Query unit tests pass; strict all-target/all-feature Query Clippy passes.
The role-composition fixture exercises all 16 original-seed subsets under all
three role preferences, checking balanced parity, stable lexical-first ordering
and unchanged complete membership. No host default changed. These checks do not
prove improved retrieval quality or complete v0 readiness.

The MCP builder composes this preference only for indexed requests without
explicit seeds. Clone preserves it; the neutral indexed builder resets both
ordering preferences. Candidate caches remain shared because this policy changes
only packing, not index contents. Source-content policy keeps its separate cache.

Standalone MCP exposes `--lexical-first-context` as an independent opt-in flag;
it enables indexed packing and composes with `--source-content-context` in either
order. Duplicate or unknown flags reject before opening the database. Neither
flag becomes implicit, and explicit seed requests keep ordinary behavior.

All nine indexed MCP tests pass, including current/retained context and the new
builder composition/cache test. Strict all-target/all-feature MCP Clippy passes.
The new discovery preference still needs response-level corpus comparison;
existing historical tests are not evidence of improved lexical-first quality.


Standalone option tests (2) and stdio process tests (4) pass, including composed
flags, unchanged initialization/search protocol and duplicate rejection before
database creation. Strict MCP all-target/all-feature Clippy passes after these
tests. Full contributor CI and the full-repository sample remain pending.

## Whole-repository result


Failed evidence bundle:
`target/context-retrieval-results/context-20261002T183005.779203Z-44646e912ab6`.
This is one release sample with a 32 MiB test-thread stack, not repeated or
controlled performance evidence. Full contributor CI is still pending.

Corrected evaluator timing labels for future runs: `stage=published` now ends
immediately after Engine indexing, before the expensive historical coverage
projection, which has its own `stage=coverage` timer. Older bundles' published
times include coverage and must not be compared as isolated indexing timings.
This is measurement plumbing only, with strict MCP Clippy verification; no
ranking, quality predicate or production behavior changes.

Ranking inspection rules out global top-k truncation before family partitioning:
`lexical_channel_scores` returns complete channel scores and `family_channels`
filters families before `.take(limit)`. The existing crowded-family oracle test
covers this invariant. Family/channel round-robin admission and label-only term
relevance remain investigation candidates, not established causes or fixes.

Accumulated contributor CI completes successfully in 202.68 seconds, including
all 17 backend conformance scenarios (42.70 seconds), architecture, migration,
extension, lint, dependency, workspace test and documentation gates. The later
timer-only diagnostic edit separately passes instrumented MCP test compilation
and strict MCP Clippy; CI's test binary was compiled before that edit. Corpus
quality remains failing independently of implementation CI.

Follow-up explicit-seed compatibility fixture passes all 24 combinations of
three role preferences, two discovery preferences, two content policies and
current/retained selection. It compares complete response JSON with the ordinary
host and verifies no candidate cache is built. Strict MCP Clippy also passes.
This verifies bypass compatibility, not indexed discovery-dependent quality.
