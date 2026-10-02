# ADR-0295: Compose bounded lexical seeds and complete packing plans from the index

Status: accepted for additive opt-in implementation; host routing unchanged.

Reuse ADR-0294's generation-pinned index and ADR-0293's composition policy.
Add `LexicalPlanRequest` with query and aggregate posting-visit budget,
`lexical_seed_plan` (maximum 32 IDs), and `lexical_packing_plan` (complete known
selected set, maximum 256 IDs, with original seeds limited to 32 and a subset
of that set). Plans contain IDs only; canonical hydration and source verification
remain the ranked compiler's responsibility.

For each code/documentation/reference/other family, retrieve exact then stemmed
ranking before family truncation, using complete-channel global statistics.
Round-robin refill preserves per-channel rank and first occurrence, then stable
primary-evidence priority. For packing, retain every selected ID, append
zero-score IDs canonically, alternate original seeds with discoveries while
preserving partition order, and apply stable primary priority again.

Reuse the diagnostic refill/balancing algorithm in focused production modules;
retain an independent complete-corpus oracle for equivalence. All eight channel
visits share one aggregate budget: repeated posting visits count each time.
Exhaustion fails before returning a partial plan. Index queries use manifest
validation only; no full canonical scans, source reads, payload hydration or
second graph traversal occurs in composition. Do not add confidence claims,
target hints, language quotas or larger candidate/token bounds.

Require independent oracle/order/membership/budget and restricted-port tests,
temporal/backend conformance and representative indexed-path quality before
any default host routing decision.

The additive ID-only composer is implemented. All 55 Query tests and strict
all-target Clippy pass, and architecture boundaries pass. Independent complete
90-node corpus scoring/composition verifies seed and discovery-balanced packing
order, exact membership and zero-score retention under matching, unrelated and
empty queries. An eight-visit singleton accepts budget eight and rejects seven;
foreign selected/original IDs fail. Restricted-port composition permits only
manifest validation and proves no payload hydration. Temporal composer/backend,
capacity boundaries, full contributor CI and representative indexed-path
retrieval remain open. No host routing is changed.

Capacity coverage now accepts 256 known zero-score selected IDs with 32 original
seeds, verifies complete deterministic retention, and rejects 257 selected IDs
or 33 originals. The existing four-backend path-index fixture compares seed and
packing plans across label/path/family edits, cached historical indexes and
durable reopen. All 56 Query tests, targeted conformance and strict Query/Turso
Clippy pass. Full contributor CI is running; representative indexed-path
retrieval remains required before host routing changes.

Full contributor CI was reverified successfully in 693.64 seconds with two
build jobs and a retained log, including backend conformance and workspace
documentation. This closes the composer CI gate, not representative indexed
retrieval quality or default routing.
