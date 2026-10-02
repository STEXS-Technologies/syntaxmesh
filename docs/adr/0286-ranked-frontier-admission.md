# ADR-0286: Honor ranked-plan priority during bounded frontier admission

Status: accepted for the opt-in ranked-seeded compiler only; verification pending.

The ranked-plan compiler introduced by ADR-0285 prioritizes seed evidence during
packing but leaves graph-frontier admission canonical-ID ordered. At a shared
node ceiling a lower-priority seed can therefore consume all expansion vacancies
before a higher-priority seed is visited. Whole-Sim exposes an unresolved
multi-seed admission failure; its helper-only source fixture proves a valid route,
but does not establish that priority ordering alone fixes the repository case.

Reuse existing bounded expansion and store-port adjacency reads. For ranked-seeded
methods only, order each frontier by descending selected-node relevance, then
canonical ID. Seed priority remains first-distinct order; admitted neighbors keep
zero relevance and later-hop ties remain canonical. Keep adjacency edge ordering,
node/hop limits, canonical output ordering and selected-endpoint edge retention.
Do not propagate priority, add guessed edges, change ordinary explicit/lexical
methods, or accept any diagnostic family allocation as a default.

This explicitly supersedes ADR-0285's canonical-frontier rule only for ranked
methods. Tests must use two seeds with opposing canonical/priority order competing
for one vacancy, verify ordinary ordering is unchanged and discarded neighbors
are not hydrated, then verify current/historical and durable backend equivalence.
Full CI and representative repository source admission remain acceptance gates.

Implementation reuses the shared expansion loop, sorting ranked frontiers by
existing selected-node priority and canonical tie break. A two-seed fixture
deliberately opposes canonical and caller order while competing for one vacancy:
ordinary admission retains the canonical seed's neighbor, ranked admission retains
the higher-priority seed's neighbor. It verifies zero neighbor relevance, depth,
exactly one hydration and only the admitted edge. All 47 Query tests and strict
Query Clippy pass. Current/historical backend saturation parity, full CI and
repository-quality evidence remain open; this does not fix cross-family packing.

The existing temporal backend fixture now publishes two source-less neighbor facts
using canonical delta/provenance machinery. Two seeds compete for one vacancy;
both priority orders must admit their corresponding neighbor at depth one with
zero relevance. Current and historical packed results match across InMemory,
File, SQLite and Turso, then remain identical after source/fact edits and durable
reopen. This scenario and combined Query/Turso strict all-target Clippy pass.
Full contributor CI is running; repository-quality claims remain unproven.

Full contributor CI subsequently passes in 172.54 seconds, including all 17
backend scenarios, mixed-feature/boundary/extension checks, migrations, strict
workspace Clippy and documentation. This closes contributor verification for
ranked frontier semantics; representative retrieval and cross-family packing
quality remain open and no default policy is promoted.
