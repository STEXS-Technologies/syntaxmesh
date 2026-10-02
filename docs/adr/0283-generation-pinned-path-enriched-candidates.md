# ADR-0283: Generation-pinned path-enriched candidates

Status: accepted for an explicit opt-in channel; quality and release gates remain open.

## Evidence

Whole-Shardline `context-20261001T221721.317496Z-uncommitted` rejects
exact/stemmed label-only seed composition: Python and Bash remain unreachable.
The independent label-plus-source-path diagnostic ranks Python's target 2nd
instead of 163rd, and Bash's target 20th instead of 32nd. These rankings do not
prove graph selection, evidence admission, cross-repository quality, or latency.

## Approach

Extend the existing immutable `GenerationIdentifierIndex` with an explicitly
selected path-enriched build mode. Preserve the existing label-only constructor
and its reference-equivalence contract. Reuse identifier normalization, rarity/
coverage scoring, postings budgets, manifest identity checks and bounded canonical
node hydration. Do not introduce a second search engine or per-query corpus scan.

Read file inventory from the same retained generation, with a separate explicit
file-count budget and checked path-byte accounting. Join node source file IDs to
that inventory; never read paths from the current filesystem or latest generation.
Deduplicate combined label/path terms per node before posting insertion. Nodes
without source locations retain label terms. A source location referencing a
missing indexed file must fail closed rather than silently produce a partial
index. Construction must reject malformed/nonadvancing inventory pages and every
exhausted budget. Canonical facts and schemas remain unchanged.

The first channel uses existing exact identifier terms. Stemming, channel fusion,
candidate allocation and default context integration are separate decisions; do
not infer their acceptance from the path diagnostic. Runtime-neutral Query remains
free of database, host, transport and workflow dependencies.

The initial `build_with_paths` constructor is implemented using a focused
`path_inventory` sibling module. All 45 Query tests and strict Clippy pass.
Dedicated tests cover exact posting/inventory budgets, label/path deduplication,
unchanged label-only behavior, retained paths after an InMemory rename,
missing-file and foreign-manifest rejection, and malformed inventory-page
validation. An independent multi-node snapshot oracle verifies scores, canonical
payloads, deterministic ties and truncation across seven questions and three
limits, including source-less nodes and overlapping label/path vocabulary.
The shared backend-conformance path fixture passes on InMemory, File, SQLite
and Turso, including durable restart. It compares exact ranked canonical payloads
across a same-ID file rename and node-label edit, checks cached historical indexes
after publication, and rebuilds both generations after reopening. Strict fixture
Clippy passes. This is a two-node fixture, not whole-repository relevance or
performance evidence. A restricted-port routing fixture now rejects every
operation except manifest verification and historical selected-ID hydration.
It verifies exact requested IDs, one manifest/batch call per successful query,
and no hydration on exhausted posting budgets. All 46 Query tests and strict
Clippy pass. This bounds Query's port usage, not opaque adapter internals or
constant-time complexity. Full CI before this additional test passes in 169.27
seconds, including all 17 backend-conformance scenarios.
Default context selection and
the original label-only constructor remain unchanged.

## Required gates

Whole-Sim `context-20261001T235340.822102Z-uncommitted` verifies exact canonical
ranking equivalence for all five path queries, but path-only top-eight seeding
admits source for only three targets at 8,192 tokens (TypeScript, JavaScript,
Quickstart), missing Python and Bash. Default retrieval remains 3/10. Reject
this seed policy for default integration; derived-index correctness does not
establish retrieval usefulness. Full CI after diagnostic corpus-oracle reuse
passes in 136.83 seconds. Whole-Shardline path-channel quality and any alternative
channel composition remain unproven.

- Compare indexed ranking with an independent complete path-enriched oracle.
- Verify deduplication, deterministic ties, missing-source handling, every budget,
  malformed inventory and foreign manifest rejection.
- Verify old-generation paths after rename/edit and durable restart across
  InMemory, File, SQLite and Turso, not only the current File-store fixture.
- Exercise graph selection and complete evidence admission on whole Shardline
  and Sim. Keep failures visible; a selected-file pass is not acceptance.
- Establish bounded query work without inventory scans or historical replay.
- Pass strict Clippy, architecture checks and full contributor CI before any
  default production integration.
