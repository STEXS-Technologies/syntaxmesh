# ADR-0327: Source-content retrieval and processing evidence

Status: proposed; implementation and acceptance evidence pending.

## Context


`QueryService::search` explicitly promises case-insensitive substring search over
canonical node names. `historical_search` promises the retained equivalent in
stable-ID order. Both delegate to store ports, and CLI search caps results at
100. Removing nodes after that cap would underfill results and could hide matching
source content. Globally suppressing External nodes would also hide legitimate
extension-produced facts.

The existing generation identifier index and lexical planner separate Code,
Documentation, Reference and Other families. Processing evidence currently joins
Other, which is not itself a declaration that a node is internal metadata.

Shardline's repository search delegates to `search_repos_with_options`, keeping
selection policy explicit rather than silently changing the basic storage view
(`crates/shardline-index/src/hub_postgres.rs`). Reuse that separation of policy
from canonical storage, not its SQL or repository-specific DTOs.

## Proposed decision

Preserve raw current/historical canonical search and direct fact reads. Keep
source-processing evidence durable, history-visible and available through the
dedicated coverage projection. No migration or rewriting existing generations.

Introduce an explicit source-content retrieval policy in the shared Query
candidate-selection path. Recognize only the SDK's reserved source-processing
namespace, not all External facts. Apply eligibility before ranking, limits and
packing; do not filter only final output. Keep accounting for canonical scanned
nodes and payloads even when they are ineligible as content candidates. Bind any
derived-index cache to the selected policy. Ordinary direct-ID evidence requests
must remain possible and must not acquire content-search semantics implicitly.

Raw search, source-content retrieval, processing completeness and semantic
confidence are separate concepts. A file's completed extraction is not proof
that every semantic claim is correct, and an absent outcome is not clean coverage.

## Required acceptance evidence


This ADR does not yet change a public contract or claim the observed gap fixed.

## Initial implementation

`GenerationIdentifierIndex::build_source_content` is an explicit opt-in builder
for the existing exact-token candidate API. It first builds under unchanged
canonical node/posting/payload limits, then uses a second bounded historical scan
to remove reserved processing IDs from postings before candidate ranking and
limits. Unrelated External namespaces remain candidates. This conservative
implementation retains the canonical population for rarity scoring and charges
all original build work; it is not a single-scan optimization. Planner/path
builders, host selection and caches are not yet integrated, so default retrieval
and raw search remain unchanged. The crowded-metadata regression passes for
pre-limit eligibility, unrelated extension retention, raw-search parity and
canonical node-budget rejection. Full integration acceptance remains pending.

The explicit `build_source_content_for_planner` builder now applies the same
selection to exact and stemmed postings, family membership and source-role
metadata. The shared crowded fixture checks both channels with a two-posting
budget and two-result limit, retaining the legitimate extension candidate. All
62 Query library tests pass, the expanded channel regression passes, and strict
all-target/all-feature Query Clippy passes. Host selection/caches, path-channel
integration, durable historical lifecycle and corpus quality remain pending.



Full-CI acceptance is not yet green. A project-local TMPDIR exposed a Git
non-repository fixture discovering the parent repository; the fixture now sets
its own child-process `GIT_CEILING_DIRECTORIES`, and its focused check passes.
The subsequent full gate passed all 20 CLI host-equivalence tests but failed
daemon `verification_reload_without_override_preserves_gap_and_fails_closed`
in the native-notification lane: the daemon kept serving a Durable generation.
The same focused test passes in both lanes when rerun alone, reporting the
expected verified-head rejection. This suggests ordering/load sensitivity but
does not prove a harmless test race; investigation and a full passing gate remain
required. No verification behavior or shutdown expectation was relaxed.



The launch process rejection fixture also passes: invalid tokenizer, unknown
option and duplicated selection option exit unsuccessfully with no protocol
stdout and without creating the requested database. All three stdio process
tests and strict MCP Clippy pass. This establishes validation-before-storage for
those cases; it does not replace the full gate or corpus-quality acceptance.

The existing sibling evaluator now accepts explicit
`SYNTAXMESH_CONTEXT_SYNTAX_POLICY=strict|record-failures` and
`SYNTAXMESH_CONTEXT_SOURCE_CONTENT_PROBE` (requires indexed-host probe), keeping
defaults unchanged. Its existing mixed-language fixture uses the selected Engine
policy and logs completed/failed/unclassified counts; benchmark metadata records
both choices. Strict MCP/xtask Clippy and all-target MCP checking pass.



The retry's live log remains at prepared ingestion beyond eleven minutes in
debug profile. A read-only process inspection observes roughly one CPU core and
1.2–1.3 GiB resident memory; its File-backed Turso database is about 82 MiB with
about 12 GiB WAL at that observation. No checkpoint, database reopening or
concurrent mutation was performed. A three-second `perf` sample retained as
`ingestion.perf.data` in retry bundle
`context-20261002T175232.884497Z-44646e912ab6` reports slice precondition checks
(7.35% and 3.20%), Turso page-cell access (5.75%), pointer alignment (5.24%) and
Turso `debug_validate_cells_core` (3.78%) among leading flat symbols. This is a
short sampled diagnostic, not a complete causal profile: debug-build overhead
is implicated, but WAL amplification and end-to-end cost remain unexplained.
Do not equate this active debug run with release performance or retrieval quality.

Logging limitation identified during the live run: the evaluator prints its
`published` marker after the full-snapshot coverage projection. Absence of that
marker therefore does not distinguish `engine.index` work from coverage reading;
the earlier "prepared ingestion" observation is not proof publication has not
committed. At a later observation the database reaches about 1.3 GiB and WAL
shrinks to about 93 KiB, but the run still has no retrieval markers. Separate
publication and coverage timing/log markers before drawing a phase-level cause
or publication-latency claim from future runs. Do not reopen the active owned
database merely to infer the phase.

The live retry subsequently reports 5,104 completed, four syntax-failed and zero
unclassified files, with `stage=published elapsed_ms=1118392`. That 1,118.392-second
measurement combines `engine.index` and full-snapshot coverage projection under
the current logging order; it is not isolated publication latency. A subsequent
three-second sample (`postingestion.perf.data`) shifts to bincode decoding:
deserialize-byte 7.29%, SliceReader read-exact result mapping 5.82%, read-u8 5.39%
and slice precondition checks 4.26%. This suggests a different decoding-heavy
phase but does not identify its exact call path. At that observation host fixture
and query markers are still absent, so no retrieval score is established.


The single release sample reports combined publication/coverage 69.097 seconds,
cold first query 5.074039 seconds, and warm queries 38.149–64.057 milliseconds.
Debug reports combined 1,118.392 seconds, first query 82.616878 seconds and warm
queries roughly 0.540–1.116 seconds. Runs overlapped, so these are diagnostic
observations, not controlled speedup or repeated production latency. Failed
bundles retain raw fixture fingerprints but finalized metadata fixture fingerprint
is null; do not present them as successful benchmark bundles. Raw evidence and
quality failure are retained rather than weakening target checks.

The existing evaluator now records target position in the actual shared seed
plan after the measured request, using its retained index; ranking is unchanged
and the diagnostic is outside the reported request timer. Selected-file rerun
bundle `context-20261002T181859.579491Z-44646e912ab6` retains the same fixture
fingerprint and 3/4 larger-budget outcome. Seed positions (zero-based) are Rust 0,
Python 1, Bash 0 and documentation 8. The heading therefore reaches the initial
30-seed plan but is absent from packed graph evidence at both budgets. The Python
target similarly reaches seeds but misses the smaller budget. Selected-corpus
packing/selection needs investigation, separate from full-corpus candidate
ranking. Strict MCP all-target/all-feature Clippy passes for the diagnostic.

Expanded diagnostics follow the actual shared selection and packing composition
without changing request behavior. Selected-corpus bundle
`context-20261002T182056.837213Z-44646e912ab6` confirms all four targets survive
graph selection. Packing positions are Rust 0, Python 1, Bash 0 and documentation
16 (heading seed position 8). The heading still misses both budgets, and Python
misses the smaller budget. This narrows the selected-corpus gap to admission after
packing ordering/excerpt cost, not extraction, initial seeds or graph-selection
loss. The planner interleaves lexical originals with graph-discovered nodes;
existing excerpts can consume the budget before later target evidence. No target
checks, budgets, production ranking or default policy were relaxed. Focused strict
MCP Clippy passes. Full-corpus stage diagnostics and a tested packing improvement
remain required; this observation does not prove the same cause for every miss.
