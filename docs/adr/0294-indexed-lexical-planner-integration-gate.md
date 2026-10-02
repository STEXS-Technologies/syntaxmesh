# ADR-0294: Preserve diagnostic semantics in indexed planner integration

Status: accepted for additive opt-in index implementation; routing remains gated.

ADR-0293 admits all five whole-Sim source targets; Shardline regression remains
pending. Its complete-corpus test oracle must not become a production per-query
scan. Reuse `GenerationIdentifierIndex`, canonical selected-ID hydration and the
ranked selection/packing compiler. Reuse the existing restricted-port test pattern
in `identifier_index/tests/query_work.rs` to prove query work boundaries.

The present index stores exact label-token postings only and truncates candidates
before hydration. Filtering that top list by family cannot reproduce the oracle:
a family candidate may lie below the global cutoff. The oracle also normalizes
English morphology before calculating stemmed-channel frequency and coverage.

Required integration invariants:

- Exact and stemmed channels use complete generation population and channel-wide
  document frequencies; family or selected-set restriction happens after scoring,
  before truncation. Do not substitute family-local rarity.
- Keep the existing exact/path APIs unchanged. An additive opt-in index capability
  must have explicit construction, posting-visit and payload accounting, immutable
  manifest identity, and fail closed on exhausted budgets or malformed input.
- Store IDs and bounded family metadata, not duplicated canonical node payloads.
  Build from generation-pinned pages; hydrate only returned IDs. No source reads,
  full node enumeration or backend lexical search is allowed during index queries.
- Selected-set reranking must retain all eligible IDs, including zero-score IDs,
  with the existing maximum-256 plan bound; verify membership before packing.
- Reuse existing family composition, discovery balancing and ranked-plan semantics.
  Ordering expresses caller intent, never fact confidence. No expected target,
  source-path hint or language quota is permitted.
- Verify exact equality against independent complete-corpus exact/stemmed family
  oracles, especially cutoff crowding, stem collisions, zero-score discoveries,
  foreign IDs, stale manifests, all budget boundaries and historical generations.
- Extend existing four-backend edit/rename/reopen scenarios and restricted-port
  work tests. Representative quality must be measured through the indexed path,
  not inferred from the test-only oracle or contributor CI.

Do not change default host routing until Shardline regression, indexed/oracle
equivalence, historical/restart conformance and host-quality measurements pass.
This gate records implementation requirements, not approval of an unfinished API.

After whole-Shardline diagnostic 5/5, implement additive
`GenerationIdentifierIndex::build_for_planner` and `lexical_candidates` APIs.
Build exact/stemmed postings and ID/family metadata together in one pinned page
walk. Charge both channels' postings and term bytes plus 33 logical metadata
bytes per node (32-byte ID and one family discriminator) to explicit budgets;
this accounting is not allocator/RSS measurement. Existing constructors keep
their exact/path behavior and accounting. Use an explicit request carrying
channel, optional family, optional maximum-256 eligible set, visit budget and
result limit. Missing planner capability fails closed. Preserve global scoring
before filtering; hydrate only matching returned IDs. Zero-score plan retention
remains the composer's responsibility, not fabricated lexical relevance.

The additive index capability is implemented. Exact/stemmed family and eligible
rankings match an independent 90-node global-frequency oracle across channel,
family, restriction, query and cutoff combinations. Stem collisions deduplicate
per document; both channel construction budgets are charged. Restricted-port
tests permit only manifest validation and selected-ID hydration during queries.
Existing constructors retain their exact/path payload limits, and all ranking
paths share canonical hydration validation. All 52 Query tests and strict
all-target Query Clippy pass. Four-backend/restart coverage, composer integration,
full contributor CI and representative indexed-path quality remain open.

The existing path-index four-backend temporal scenario now also checks exact
and stemmed rankings across all families and an eligible-ID restriction. It
changes a node from code to documentation while editing labels and renaming the
file, verifies old cached indexes remain pinned, and rebuilds both generations
after File/SQLite/Turso reopen. The scenario, all 52 Query tests and strict
Query/Turso Clippy pass. Posting visits are charged even when family filtering
would discard every match. Full contributor CI is now running; composer and
representative indexed-path integration remain open.

Full CI initially rejected a lint allowance in the restricted-port helper;
it now uses the existing error-returning check helper. The independent extension
fixture lockfile is refreshed for the Query dependency. Later verification
encountered filesystem exhaustion and linker failures, not a passing full-CI
result. Crate-scoped Cargo cleanup removed only regenerable build artifacts;
`CARGO_BUILD_JOBS=2 cargo make ci` is running with restored disk headroom.
No indexed stores or benchmark evidence were removed. Full CI remains unverified
until that run is terminal and successful.

The resource-recovered `CARGO_BUILD_JOBS=2 cargo make ci` run completes
successfully in 290.57 seconds, including all 17 backend conformance scenarios,
architecture/migration/extension checks, strict workspace Clippy, dependency
policy, workspace tests and documentation. This closes contributor verification
for the additive index capability, not composer integration or indexed-path
repository-quality acceptance. Default host routing remains unchanged.
