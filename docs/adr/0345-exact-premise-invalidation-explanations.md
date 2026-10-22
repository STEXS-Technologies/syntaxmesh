# ADR-0345: Exact premise invalidation explanations

Status: implemented for the pure scoped dependency index.

Add scoped dependency lookup returning a stable claim-ID map of exact changed
premise versions responsible for each hit. Bound total lookup/posting work,
affected claim count and explanation-reference count; any exhaustion rejects the
whole result. Preserve the cheaper existing ID-only lookup for callers not needing
explanations. Reuse canonical witness postings, not replay or causal guesses.

Query exposes range explanations through the same validated closed-version scan
as ID-only invalidation. An explicit evidence budget bounds returned references;
zero budget rejects before store reads. Existing ID-only callers avoid allocating
explanation maps. This additive optional Rust API has no host wire contract yet.

This explains direct dependency matches, not why a source changed, recursive
propagation, durable retraction or a complete historical causality proof.

Verification: sixteen inference unit tests and real Rust-source integration pass.
The explanation map matches an independent witness-intersection oracle and its
keys match ID-only lookup. Work, claim and evidence exhaustion reject. Strict
all-target inference Clippy passes.

Query range delivery now passes all four backend lifecycle fixtures against the
exact closed-version explanation oracle. Zero evidence budgets reject. Fourteen
malformed-feed cases reject through both ID-only and explanation paths; strict
Query Clippy passes. Host wire delivery and durable retraction remain open.

Evidence-boundary coverage accepts the exact total claim/premise reference count
and rejects one reference less. Multiple reasons retain one map entry per claim.
Empty scoped indices return no explanations but still reject foreign selection;
zero evidence budgets reject. The sixteen-unit-test inference suite and strict
all-target Clippy pass with these additional boundary assertions.
