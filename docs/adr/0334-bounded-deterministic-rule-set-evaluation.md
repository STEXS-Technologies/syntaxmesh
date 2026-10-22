# ADR-0334: Bounded deterministic rule-set evaluation

Status: implemented for independent rules.

The optional reasoning plane must support extension-owned collections of rules,
not just one hard-coded rule. Add pure `evaluate_rules` with an explicit rule
count limit and the existing fact/depth/claim/edge-visit limits. Edge visits and
output count are shared across the complete set, not reset per rule. Reject
duplicate rule identity/version pairs and return no partial set on any failure.
Sort rules by identity/version and results by claim identity for deterministic
input-order-independent results. Single-rule evaluation remains equivalent.

This executes independent supported path/join rules over canonical premises.
It is not recursive derived-claim chaining, an ontology registry, or durable
publication. Those remain separate roadmap requirements.

The optional Query adapter now exposes `infer_rules`, loading authoritative
active premises once for the complete set. Existing `infer` delegates through
the same path. Nine pure unit tests plus Rust extraction integration pass;
shared four-backend fixtures verify retained two-rule results and current
retraction. Strict inference, Query and SQL all-target Clippy passes.
