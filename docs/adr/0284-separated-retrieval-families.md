# ADR-0284: Separate retrieval families before default ranking integration

Status: proposed; no public contract or default policy accepted.

## Evidence

Whole-Sim `context-20261002T004040.155335Z-uncommitted` completes all ten
natural-language measurements and fails its required-target quality gate.
Default retrieval remains 3/10. Its complete-corpus diagnostics show:

- Python: stemmed rank 33 overall, 7 among the existing definition candidates,
  versus exact definition rank 60. Leading definitions include tests and document
  examples. Reference labels can contain large code expressions.
- Bash: stemmed rank 4 overall, 3 among definitions, versus exact definition rank
  58. Documentation chunks and an external anchor lead the actual Script.
- TypeScript: stemmed definition rank 65, exact definition rank 25; leading
  definitions are predominantly documentation chunks.

The path-only seed policy scored 3/5; the mixed 2/4/2 policy scored 2/5 and
deduplicated Python/Bash to four seeds. Neither is accepted. Increasing a global
graph budget alone is not supported: the mixed Bash traversal finishes at 27
nodes without reaching its target.

## Direction

Focused real-source helper-route diagnostic (one unchanged Sim
`scripts/generate-docs.ts`, fingerprint
`cb32e45aa56bba47a0bd45066f3d62d959c50086ca0f1fa8f6d1b3a986323383`)
indexes in 849 ms and confirms a canonical `Calls` relation between
`generateBlockDoc` and `generateMarkdownForBlock`. A helper-only, two-hop
neighborhood reaches the target at depth one with both 64 and 256 nodes: the
64-node result truncates, while 256 admits all 114 nodes. This isolates a valid
source-file route; it is not proof of identical whole-repository resolution.
The whole-Sim 32-seed run still excludes the target. Inspect multi-seed admission
ordering/capacity next rather than inventing a missing edge or increasing defaults.
The dedicated ignored Rust diagnostic reuses the existing external-source fixture
and Query neighborhood; its explicit run and strict MCP Clippy pass. No analyzed
runtime is launched and no candidate or host policy changes.

Inspection of the completed ranked-family artifact identifies Python's desired
method as seed 18, despite its fourth place in the stemmed code family. The pack
admits a 16,613-byte dashboard expression (lines 860–1210), 2,810-byte log-event
span and 2,800-byte workflow-details span alongside retry tests and SDK docs,
while omitting the 2,670-byte canonical method span. This is not lost ranked-plan
priority: equal-channel composition places heterogeneous reference evidence
ahead of that method. Reference/display-content relevance and source cost must
not be mistaken for symbol-definition relevance. A flat round-robin is therefore
not a sufficient cross-family packing policy. Do not simply hard-filter references:
earlier Shardline evidence shows occurrence routes can be necessary for Rust.

TypeScript's `generateMarkdownForBlock` helper is seed one, yet the desired
`generateBlockDoc` is absent from the final capped selection. This identifies a
separate graph-admission failure; it does not prove the extractor omitted the
target or that a canonical relation is absent. Diagnose bounded expansion and
reference/definition routing separately from source-budget allocation. Do not
increase all bounds or allocate quotas to these known targets.

The 32-seed globally weighted exact/stemmed family round-robin experiment with
ranked packing, whole-Sim `context-20261002T012914.538931Z-uncommitted`, is
terminal at 3/5 source admission: JavaScript, Bash and Quickstart hit; TypeScript
is not selected and Python is selected but not packed. Default quality remains
3/10. Reject this equal-channel allocation for default integration. Family
separation/global rarity and the additive ranked-plan compiler are compositional
building blocks, not evidence that this allocation generalizes from selected-file
5/5. Next diagnosis must distinguish candidate admission from packing priority.

Whole-Sim statistics-scope comparison
`context-20261002T011024.747981Z-uncommitted` is terminal with ten default
measurements and the required-target quality failure. Publication took 369.715
seconds for the unchanged 2,572-file fingerprint. Global versus family-local
stemmed target ranks are TypeScript code 35/34, Python code 4/31, JavaScript
code 1/1, Bash code 1/2 and Quickstart documentation 1/1. Exact ranks are
respectively 6/21, 38/43, 1/1, 33/33 and 2/1. Family-local rarity is not supported
as a replacement: Python materially regresses and TypeScript gains little.
Use complete-generation rarity for the next separated-family prototype;
retaining family-local reports does not accept a public/default policy. These
are rank measurements, not family-seeded source-admission evidence. Default
retrieval remains 3/10; allocation and representative Shardline gates stay open.

Whole-Sim family-only diagnostic
`context-20261002T005340.222311Z-uncommitted` is terminal. Code-family stemmed
ranks are TypeScript 35, Python 4, JavaScript 1 and Bash 1; Quickstart is first
in the documentation family. Python's code leaders are two rate-limit tests,
the TypeScript SDK retry method, then the Python SDK retry method. Family
separation therefore retains real cross-language competitors rather than
silently selecting the known target language. Default retrieval remains 3/10.
These rankings support family separation but do not prove graph reachability,
source admission, a production allocation, or filtered-channel IDF semantics.
The eight-ID round-robin preview is not accepted merely because Bash ranks first:
it provides too few code slots to directly include Python's fourth-ranked target.
Full contributor CI for the diagnostic helpers passes in 130.96 seconds.

Reuse existing identifier normalization, exact ranking, diagnostic Snowball
normalization, canonical node kinds, generation-pinned postings, manifest checks
and bounded hydration. Do not introduce another search engine or infer edges.

Evaluate code symbols/scripts/files separately from documentation and reference
occurrences. The current “definition” selector conflates code with long document
chunks; do not use it as a synonym for code definitions. Preserve document and
reference channels so architectural rationale and indirect symbol routes remain
available. Large display expressions must not silently become equivalent to
symbol-name vocabulary; any separate content field needs its own explicit scope
and budget.

Keep each channel ordered by its score and canonical-ID tie break. Deduplicate
without discarding that ordering, refill vacant slots from remaining candidates,
and enforce one global distinct-candidate bound. Do not slice ID-sorted unions
as though they retained relevance order. Allocation/fusion weights remain open;
do not hard-code a replacement based on these five known targets.

First evaluate the family separation using existing test-only corpus helpers.
Only after representative evidence should a generation-pinned Query contract be
designed. Query/core/DTO/protocol remain runtime-neutral; no database, transport,
workflow, host, or local inference dependency belongs there. Canonical facts,
producer versions and retained history are unchanged by a derived ranking view.

## Gates

Next diagnostic prototype: compose complete exact and stemmed rankings within
each of the four canonical families using global rarity, with relevance-preserving
round-robin duplicate refill and the existing 32-distinct-seed request ceiling.
This is an explicitly named equal-channel experiment, not accepted allocation
weights. Keep the existing 64-node graph and 8,192-token packing limits unchanged;
report actual seed IDs, reachability and source admission. A dedicated probe flag
must require corpus/index/graph probes and reject competing seed policies. No
new production Query API or default routing is authorized by this experiment.

Family diagnostics now report both exact/stemmed rankings with complete-generation
rarity and a separately named `family_local_statistics` comparison computed from
all family members (including nonmatching labels), using the existing scorer
without cloning canonical nodes. A source-less adversarial fixture has eight
alpha code symbols, one beta code symbol and eighty beta documentation chunks:
global statistics rank beta ninth in code; family-local statistics rank it first.
This proves the two contracts are not interchangeable, not that either is better
on real repositories. Empty families are covered. No preview seed selection or
default query behavior changes. All 21 nonignored MCP unit tests, stdio integration
and strict MCP Clippy pass. Full contributor CI subsequently passes in 130.05
seconds, including all 17 backend scenarios; repository-scale scope comparison
remains open.

Diagnostic refill now has exhaustive small-alphabet sequence checks adapted
from Shardline's lifecycle sequence-invariant testing approach: 40 sequences
per input channel, 1,600 overlapping/empty channel pairs and seven limits verify
complete membership, uniqueness, capacity, prefix consistency and singleton
first-occurrence relevance order. This introduces no property-test dependency
and does not establish a production allocation or retrieval-quality improvement.
All 20 nonignored MCP unit tests and the stdio integration pass; strict MCP
Clippy passes. Full contributor CI has not been rerun for this test-only addition.

- Independently verify complete ranking, deterministic ties, source-less facts,
  retained generation identity and every construction/query budget.
- Verify code/document/reference family classification explicitly, including
  Script, Module, DocumentChunk, External and unresolved/ambiguous references.
- Test overlapping channels, duplicate refill, exhausted channels and strict
  global limits without losing canonical payload identity.
- Measure graph reachability and complete source admission on whole Sim and
  Shardline, preserving documentation cases and all failures.
- Distinguish vocabulary/selection failures from graph resolution and token
  packing failures; do not create guessed call edges to improve retrieval.
- Require an ADR update before a public Query contract or default host policy,
  then strict Clippy, boundary checks and full contributor CI.
