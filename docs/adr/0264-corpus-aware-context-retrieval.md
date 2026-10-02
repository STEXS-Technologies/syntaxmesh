# ADR 0264: Corpus-aware context retrieval investigation

Status: proposed; no production contract change authorized by this ADR yet

Whole-Sim evidence separates candidate truncation from downstream ranking.
Increasing a stable-ID-ordered substring page is not a complete retrieval design.
Reuse the useful Graphify principles without its Python/NetworkX storage:
generation-scoped corpus term frequency, multi-term coverage-aware ranking,
and a candidate prefilter that preserves potentially matching nodes.

Inspected reference: installed Graphify `serve.py`, `_compute_idf` and
`_score_nodes` implementation. Frequency is computed over every node label;
its trigram candidate prefilter preserves matches rather than arbitrarily
discarding them by node ID. Port ideas, not source code or its runtime.

Requirements before choosing implementation:

- Keep canonical substring search behavior unchanged; context retrieval can
  have a separate explicit port if required.
- Bind indexes and statistics to generation and resolver/extractor policy;
  current and historical context must use identical logical scoring.
- Do not call capped candidate counts exact document frequency.
- Retain explicit query/scan/output limits and deterministic tie-breaking.
- Keep pure contracts free of database, host, and workflow dependencies.
- Prefer existing database indexes or a maintained Rust crate to a new custom
  text-search engine; assess incremental deletion, retained generations, and
  migration/rebuild behavior before adoption.
- Validate against whole Sim plus independent repositories, preserve exact
  target metrics separately from answer-bearing evidence coverage, and reject
  regressions even when unit tests pass.

Open: bounded candidate selection/index representation, exact statistics cost,
identifier token normalization, morphology policy, history index retention,
and indexed update/publication cost. This ADR is not proof these are implemented.

Test-only full-corpus probe implemented with integer logarithmic rarity and
query coverage (not an exact Graphify score port). Its synthetic rare-multi-term
test and strict MCP Clippy pass. Whole-Sim probe scans 87,486 node labels:
TypeScript target rank 55, Python rank 258, Quickstart rank 1. This improves
candidate visibility for some cases but does not establish complete retrieval;
Python remains outside the 64-candidate budget. Raw evidence:
`context-20261001T125117.643344Z-uncommitted`. Production queries stay unchanged.
Whole-graph probe reads warm caches; do not use its timings as production latency.

Follow-up whole-Sim comparison, raw bundle
`context-20261001T125420.945437Z-uncommitted`: all-fact / declaration-document
target ranks are TypeScript 55/24, Python 258/73, JavaScript 1/1, Bash 3/2,
Quickstart 1/1. Restricting the seed domain therefore still misses Python at a
64-candidate bound, and silently dropping reference facts is not an accepted
solution. The comparison uses all-fact corpus statistics in both domains.

The existing isolated Rust Turso FTS fixture establishes token-versus-substring
behavior. Assess identifier normalization in that fixture before choosing a
separate context-candidate index; do not change canonical substring search or
assume native tokenization recognizes camelCase identifiers.
The extended locked probe passes: `block` matches only the normalized
`generate block doc` label, not `generateBlockDoc`; `retry` matches
`execute_with_retry`. Assertions retain this evidence as a regression gate.
Therefore any token-index approach requires explicit identifier normalization
and cannot replace the canonical arbitrary-substring port.

The isolated temporal FTS probe also passes scope/validity-before-LIMIT checks.
A foreign-scope row with the lowest ID cannot consume the one-result budget;
closing a local label's interval preserves its historical match, excludes it
from the new generation, and exposes its replacement only after introduction.
It reuses native FTS plus SQL predicates and the existing interval-storage idea,
not a new text engine. This establishes functional filtered candidate lookup
only: it does not prove index-backed predicate pushdown, bounded scan work,
generation-scoped ranking statistics, durable restart, or production suitability.
Native corpus scoring across retained/foreign rows must not be mistaken for
generation-specific relevance. No production dependency or schema changed.

Further locked-probe evidence: EXPLAIN reports `QUERY INDEX METHOD fts` plus
`USE SORTER FOR ORDER BY`. Scoped SQL term counts correctly exclude foreign and
retired rows on the fixture. More importantly, inserting one unrelated foreign
row changes the same retained local row's native `fts_score` from
0.44713860750198364 to 0.640724241733551. Therefore native score over a shared
temporal FTS table is unsuitable as the historical-context ranking authority:
future/foreign corpus changes can alter a pinned generation's relevance.
Use native FTS only as a possible candidate accelerator, with independent
generation-scoped statistics/scoring, or a separately scoped derived index.
This is demonstrated score contamination, not a latency measurement.

Whole-Sim normalized tops include large expression references and a directly
relevant Python retry test; exact target rank alone does not establish absence
of answer-bearing evidence or whether graph expansion reaches the function.
Bash's query uses `guardrail` while its path contains `guardrails`. A test-only
crates.io rust-stemmers 1.2.0 characterization reuses Snowball English stemming
after identifier normalization and confirms guardrail/guardrails,
execute/execution, retry/retries share stems. The targeted test and strict MCP
Clippy pass. This dependency is dev-only; no production stemming or index policy
is approved. Cross-corpus ranking, overstemming and non-English behavior require
evaluation before adoption. Upstream API: https://docs.rs/rust-stemmers/1.2.0/rust_stemmers/.
The opt-in corpus evaluator now compares exact stemmed tokens over its already
loaded diagnostic nodes, using complete corpus frequencies and the same integer
rarity/coverage principle. A plural multi-term fixture ranks the expected
loadTransactionLog first; both corpus-probe tests and strict MCP Clippy pass.
Whole-repository evaluation is running; no relevance acceptance is claimed from
the synthetic fixture. This full-corpus diagnostic remains test-only and is not
a bounded production retrieval implementation.

Completed stemming run `context-20261001T134829.609633Z-uncommitted`: exact
normalized / stemmed target ranks are TypeScript 46/242, Python 222/33,
JavaScript 1/1, Bash 182/4, Quickstart 3/1. Stemming addresses the demonstrated
Python/Bash morphology gaps but regresses TypeScript substantially. Reject
unconditional stemming as the production replacement. Investigate separate exact
and stemmed candidate channels with an explicitly evaluated merge policy; do not
silently enlarge the user's candidate budget or label a union as a 64-item set.
The production context acceptance still fails; these ranks measure diagnostic
candidates, not packed answer-bearing evidence or graph-expanded retrieval.

The test-only corpus probe now also measures equal-weight reciprocal-rank
fusion of complete exact and stemmed channels, with offset 60, fixed-point
integer scores and stable NodeId ties. It reuses the same normalized-token
scorer rather than adding a production retrieval implementation. The plural
multi-term fixture checks fused target visibility and repeat determinism;
targeted tests, strict MCP Clippy and formatting pass. Whole-corpus acceptance
is still required: this experiment does not authorize fusion, change the
64-candidate budget or establish bounded candidate-generation cost.

First whole-Sim fusion attempt `context-20261001T140433.089115Z-uncommitted`
failed during initial workflow publication with tmpfs `pwritev: quota exceeded`,
before any candidate metrics. The host's user tmpfs quota is approximately
25 GiB; filesystem free space alone did not reveal this limit. This bundle is
infrastructure-failure evidence only, not a fusion-quality result. Retry uses a
dedicated disk-backed TMPDIR without deleting unrelated temporary data.

Disk-backed whole-Sim retry completed in
`context-20261001T140601.449126Z-uncommitted`, with the same 87,486-node corpus.
Exact / stemmed / equal-weight reciprocal-fused target ranks:
TypeScript 46/242/82, Python 222/33/55, JavaScript 1/1/1,
Bash 182/4/24, Quickstart 3/1/2. Reject this fusion as a production policy:
it makes TypeScript fall outside 64 candidates while bringing Python/Bash
inside. Production retrieval independently still misses TypeScript, Python
and Bash. The test-only scorer has no label-length normalization; large
expression references appear among fused top results. Assess field/length
effects and answer-bearing evidence coverage before further policy adoption,
rather than tuning weights to these five targets or increasing their budget.

Length-normalization investigation reuses crates.io Tantivy 0.26.2's public
`Bm25Weight`/`Bm25StatisticsProvider` and fieldnorm encoding. The installed Turso
FTS implementation already uses Tantivy; no sibling project BM25 implementation
was found. Tantivy is added only as an MCP dev-dependency, with default features
disabled. The diagnostic supplies corpus-local document frequencies and compares
exact/stemmed unique-term labels using binary term frequency and unique-term
length, explicitly not raw-token BM25. No persistent index, production dependency,
schema or retrieval policy changes. A focused fixture checks a concise label
against a longer label with the same query terms and empty-query behavior.
Compilation/verification and whole-corpus quality evidence remain required.
The focused corpus-probe tests, locked offline test compilation, strict MCP
all-target/all-feature Clippy and formatting now pass. The disk-backed whole-Sim
evaluation has been started; no relevance acceptance follows from unit tests.

Completed whole-Sim unique-term BM25 comparison:
`context-20261001T141402.544102Z-uncommitted`. Exact/stemmed BM25 ranks are
TypeScript 12/34, Python 272/103, JavaScript 12/12, Bash 171/3, Quickstart 1/1.
Length normalization improves TypeScript versus the earlier exact/stemmed
coverage scorer but worsens Python, whose stemmed rank was 33 previously.
Reject this scorer as a blanket production replacement. It sums per-term BM25
weights with no extra coverage multiplier and uses unique terms, not repeated
token counts; those differences remain distinct from a production text index.
Production acceptance independently still misses TypeScript/Python/Bash.
Check the unchanged diagnostic on an independent whole repository before further
policy changes; do not optimize only for the five Sim targets.

Dependency acceptance failed: `cargo deny --locked check` reports
RUSTSEC-2026-0253 for Tantivy's transitive `lru 0.16.4`. The local RustSec advisory
requires >=0.18.2, while Tantivy 0.26.2 requires the incompatible 0.16 series;
crates.io currently lists 0.26.2 as latest. Do not add an advisory exception or
claim the contributor gate remains green with this new dependency. The running
Shardline diagnostic retains its original build provenance; after it finishes,
remove this experimental dependency/scorer and retain the measured rejection
evidence. Any replacement scorer must pass dependency policy before adoption.

Independent whole-Shardline run completed:
`context-20261001T141928.826232Z-uncommitted`, 206,395 nodes. Exact/stemmed/fused
ranks are Rust webhook 31/38/32, Python metadata 19/32/19, Bash query 22/22/22,
benchmark heading 3/3/3, benchmark rationale 1/2/2. Unique-term BM25 exact/stemmed
ranks are respectively 24/24, 2/3, 6/6, 1/1, 3/26. These diagnostic targets
all fit 64 candidates here; unchanged production context nevertheless misses
Python metadata. Cross-corpus results therefore do not resolve the Sim regressions
or prove packing/graph-expansion correctness. The rejected Tantivy diagnostic
code and direct dev-dependency have now been removed; raw bundles retain the
experiment and its build provenance. No advisory exception or custom BM25 formula
was introduced. Dependency-policy revalidation follows the removal.
After removal, `cargo deny --locked check` passes advisories/bans/licenses/sources;
targeted corpus-probe tests, strict MCP Clippy and formatting pass. Existing
cfg_block license and yanked yoke-derive warnings remain distinct from the removed
lru advisory. This is targeted revalidation, not a fresh complete workspace gate.

The opt-in sibling evaluator now has a separate explicit-target-seed probe using
the existing MCP API, unchanged 64 candidates/one hop and 8,192-token budget.
It reports target source-evidence visibility separately and validates generation
and token bounds. Natural-language acceptance still runs unchanged and can fail;
seeded success is diagnostic isolation of candidate selection from packing/source
reading, not a relaxed quality gate or an alternate product workflow.

Whole-Sim seed probe completed in
`context-20261001T144912.124844Z-uncommitted`: all five targets return source
evidence when explicitly seeded, within 8,192 tokens and the pinned generation.
Natural-language acceptance independently still misses TypeScript/Python/Bash.
This isolates candidate selection as an actionable failure for these fixtures;
it does not prove arbitrary targets or lower budgets pack correctly. The prior
prebuild failed on a probe variable scope error and supplies no retrieval evidence;
the corrected probe passes strict MCP Clippy.

Independent Shardline seed probe completed in
`context-20261001T145109.954228Z-uncommitted`: all five explicit targets return
source evidence within generation/token bounds, including Python metadata;
unchanged natural-language acceptance still misses Python metadata. Candidate
selection is therefore the actionable failure in both measured corpora. This
does not authorize an increased candidate cap or establish a ranking policy.

The existing test-only reciprocal merge now additionally reports zero-offset
fusion, preserving the offset-60 result for direct comparison. This tests stronger
preference for channel-leading candidates without changing either channel scorer,
the user candidate budget or production context. It remains a complete-corpus
diagnostic, not bounded production retrieval. Cross-corpus results and packing
acceptance are required; a favorable five-target result alone must not select a
production policy.

Completed zero-offset comparison:
`context-20261001T150313.984749Z-uncommitted`. Zero-offset ranks are TypeScript
73, Python 56, JavaScript 1, Bash 8, Quickstart 2. Reject this policy too:
TypeScript remains outside 64 candidates. No independent acceptance claim is
needed to reject a demonstrated failure on the existing required corpus.
Stop offset tuning on these five targets; label-only channel fusion has not
established adequate selection. Inspect richer candidate fields/graph signals
and an explicitly scoped indexed retrieval boundary before another production
policy proposal. Natural-language acceptance remains unchanged and failing.

Next test-only comparison enriches node labels with their existing owned/source
file's normalized path, obtained from generation-pinned historical file pages.
It changes no canonical node/ID, parser, dependency, DTO or production ranking.
Missing file inventory entries leave labels unchanged; pagination validates
bounds, nonempty continuation and increasing FileId. The full-corpus diagnostic
still materializes labels/inventory and must not be presented as bounded retrieval.
Focused coverage verifies ownership enrichment, absent files and unchanged input
nodes; targeted tests and strict MCP Clippy pass. Whole-corpus relevance acceptance
remains required.

Completed owner-path comparison:
`context-20261001T151010.606860Z-uncommitted`. Exact/stemmed ranks are
TypeScript 269/449, Python 1,131/70, JavaScript 1/1, Bash 231/22,
Quickstart 3/1; offset-60 fusion ranks are 356/134/1/60/2 respectively.
Reject concatenating paths into every node label: it floods the candidate
population with shared file vocabulary and regresses required targets beyond
64 candidates. This diagnostic does not justify a production change. Separate
field scoring or file-level selection needs independent evidence; no ranking
policy or candidate cap is changed by this result.

Next bounded graph comparison reuses Graphify's seed-then-expand principle and
the existing historical-neighborhood API, not a second traversal implementation.
Opt-in `SYNTAXMESH_CONTEXT_GRAPH_SEED_PROBE=1` with corpus/index probes selects
the top eight exact-token candidates (without target knowledge) and expands
two hops, capped at 64 nodes, 256 edges, 1,024 incidences and 1 MiB output.
It reports target depth, truncation, work and duration. These fixed diagnostic
limits do not change production context selection or natural-language acceptance.

Whole-Sim bundle `context-20261001T194252.134576Z-uncommitted` reaches
TypeScript at depth 1 and JavaScript/Quickstart at depth 0. Python/Bash remain
absent. All five traversals hit the 64-node cap, report truncation and scan
121–459 incidences; durations are 21–51 ms locally. This supports inspecting
graph expansion for TypeScript, not a general production policy. Next pass
feeds the identical top-eight IDs to the existing context API at two hops,
64 candidates and 8,192 tokens, checking actual target source evidence and
generation/token bounds independently of the unchanged natural-language gate.

Context-pack follow-up `context-20261001T194637.775438Z-uncommitted` meets
generation/token bounds but retrieves only JavaScript and Quickstart. TypeScript
is graph-reachable in the independent neighborhood yet absent from the context
pack (8,190 tokens); Python/Bash also miss. Reject top-eight exact seeds plus
two-hop existing-context expansion as a sufficient policy. Graph reachability
is not answer-bearing evidence inclusion; inspect selection/packing differences
before changing production ranking. Natural-language acceptance remains failing.

Code inspection identifies a selection confounder: context merges the eight
explicit IDs with ordinary lexical results before expansion, potentially
filling all 64 slots. The independent neighborhood starts only from eight IDs.
Next diagnostic uses the existing API with an empty lexical query and those
same IDs (`selection=explicit_only` in logs), preserving two-hop/token/candidate
limits. This isolates expansion without changing public contracts or default
semantics; seed retrieval still uses the original natural-language query.

Seed-only bundle `context-20261001T195038.549889Z-uncommitted` still includes
source evidence only for JavaScript/Quickstart; TypeScript/Python/Bash miss.
All requests remain generation/token bounded. Removing lexical merging is not
sufficient; next diagnostic records target item kinds and warning codes, without
source contents, to distinguish absent selection from source-span packing loss.
