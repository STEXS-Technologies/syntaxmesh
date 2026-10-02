# ADR-0291: Separate packing-plan capacity from graph-seed capacity

Status: accepted additive boundary; contributor verification complete.

Reuse ContextRequest, canonical node hydration and the shared ranked compiler.
Add current/historical ranked_plan_context and ranked_plan_context_selection
methods. For these methods seed_nodes is an ordered packing plan, bounded by
max_candidates (at most 256 raw entries), not the existing 32 graph-seed limit.
Require a nonempty plan and max_hops zero. Duplicate IDs retain first-distinct
priority. Unknown IDs fail closed; source hashes and generation verification use
the existing compiler. Priority represents caller intent, not graph-derived fact.

Existing lexical/explicit/ranked-seed APIs keep their validation and semantics.
No traversal, lexical lookup or implicit priority propagation occurs for plans.
The planner may retain all 64 selected IDs without raising candidate/token bounds.
This API is not a retrieval policy or a claim of better repository quality.
Require input bounds, identity/priority retention, current/history parity and
backend/restart checks before any host policy promotion.

The additive boundary is implemented in a focused packing_plan module, with no
implementation in lib.rs. All 49 Query tests and strict all-target Query Clippy
pass. The extended shared context fixture verifies a 40-raw-entry/two-distinct-ID
plan retains first-distinct priority and exact current/historical packs; old
ranked-seed APIs reject it. Zero-hop, raw-capacity, empty and 257-entry rejection
checks cover selection and packing, current and historical. This does not yet
prove retention of more than 32 distinct IDs; that fixture, four-backend/restart
coverage, full contributor CI and planner-quality measurements remain open.

The existing temporal backend scenario now includes 40 distinct synthetic
source-backed facts and one duplicate plan entry. Selection retains every ID with
the expected first-distinct priority; packing retains all source attributions and
an exact serialized-byte counter. Packs match across InMemory/File/SQLite/Turso,
current/history, source edits and durable reopen. The targeted scenario and strict
Query/Turso all-target Clippy pass. This closes distinct-capacity/backend coverage,
not parser-extraction or real-repository relevance. Full contributor CI remains
required before integrating a diagnostic planner with this boundary.

Full contributor CI completes successfully in 176.93 seconds, including all 17
backend scenarios and contributor gates. This verifies the additive compiler
boundary and 40-distinct-node retained fixture; planner quality remains unproven.
