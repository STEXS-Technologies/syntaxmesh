# ADR-0318: Reuse complete lexical scoring across family partitions

Status: focused implementation, contributor CI and single-corpus release parity verified.

Release bundle `context-20261002T144755.768422Z-uncommitted` passes in 84.59
seconds parent time on the unchanged 808-file Shardline fingerprint
`4419eb47c3925327b409c61efc391c8886aca746badb7fe664e4daae8c143795`.
Index counts remain 202,664 nodes, 2,067,222 postings and 72,907,891 payload bytes.
All six larger-budget and four smaller-budget target checks pass, with identical
packed token counts to ADR-0317's six-case baseline. Publication is 42.649
seconds; cold migration context is 2.587879 seconds, including 2.555668 seconds
index construction. Warm larger-budget queries span 36,207–42,632 microseconds,
versus the previous single sample's 42,188–65,173. This is limited observed
parity and timing, not repeatability, causal speedup, or relevance acceptance.

Full contributor CI passes in 184.90 seconds, including all 17 backend/restart
scenarios (42.58 seconds), all-feature workspace tests, strict Clippy,
architecture/extension/migration checks and documentation. The accepted advisory
policy remains unchanged. Matching-input release evaluation is the next gate.

All 59 Query unit tests and strict all-target/all-feature Query Clippy pass.
The crowded-family regression verifies a documentation match below 300 higher
scoring code matches is retained by the family plan while ordinary global
top-256 lookup excludes it. Existing independent-oracle, aggregate budget,
foreign-ID and capacity tests pass. Restricted-store tests retain zero canonical
payload hydration and observe four rather than sixteen manifest reads across
the seed/packing pair. No production ranking or target-retention change is
claimed. Full contributor CI and matching-input corpus evaluation remain open.

The family planner repeats the same global-rarity exact/stemmed scoring for
four families. Reuse its existing scoring implementation once per channel,
then filter the complete sorted results into the existing eight queues.
This borrows the existing generation-scoped index, not a new retrieval service.
Sibling review found no suitable local reranker (ADR-0316).

Keep public contracts, channel/family order, tie ordering, candidate membership,
and conservative aggregate posting-budget charges unchanged. Charge each logical
family/channel visit even when physical scoring is reused, including rejection
of a zero remaining budget. Filter before per-family truncation; never truncate
the global result first. Retain generation/manifest identity checks and eligible
ID validation. Existing oracle and budget tests must pass; add a crowded-family
case whose documentation candidate lies beyond the global top 256.

This reduces repeated work, not small-budget admission pressure. No latency,
relevance improvement or default promotion is claimed without measurement.
