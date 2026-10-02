# ADR-0299: Permit an empty explicit packing plan without lexical fallback

Status: accepted for additive empty-plan support.

An indexed query may correctly return zero seeds. Its host should not invent a
seed, scan the canonical corpus as fallback, or report invalid user input.
Allow empty `RankedPlan` requests in current/historical selection and packing.
An empty plan selects no nodes and performs no lexical lookup or expansion;
the existing compiler still validates generation, request limits and exact
serialized token budget. A too-small output budget remains an error.

Keep empty ExplicitOnly and RankedExplicit seed requests invalid: those APIs
express a required starting set for traversal, unlike a complete selected-set
packing plan. Preserve zero-hop and raw-plan capacity checks for RankedPlan.
Use empty ranked-plan packing directly in the opt-in MCP path when the indexed
seed plan is empty. Test current and historical empty packs with a query that
would otherwise match facts, proving there is no implicit lexical fallback,
and add an absent-query host fixture. No default routing changes.
