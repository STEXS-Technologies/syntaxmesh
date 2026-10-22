# ADR-0337: Explicit ontology version registry

Status: implemented for explicit version existence.

Add a pure bounded registry of explicitly supplied ontology namespace/version
pairs. Reject duplicate or empty entries. An opt-in registered evaluation checks
every rule binding against this registry before inference; unknown namespaces or
versions reject, never silently fall back. Multiple registered versions may
coexist, while ADR-0336 still prohibits mixing versions of one namespace within
one selected rule set. Bound registry construction, rule count and binding visits.

This is version-existence validation only. Registration does not authenticate
ontology sources, validate concept or relation signatures, infer compatibility,
or persist registry state. Ordinary inference remains available for callers
whose explicit configurations are verified elsewhere. Query's registered path
validates before loading retained facts. No core/store/runtime dependency changes.

Verification: eleven inference unit tests plus Rust extraction integration pass.
Registered Query equivalence, unavailable-version rejection and binding-budget
exhaustion pass in the shared four-backend lifecycle fixture. Strict inference,
Query and SQL all-target Clippy passes. Full workspace CI completed in 179.22
seconds for the preceding inference slice; this registry has targeted verification
only and is not covered by that earlier workspace-test snapshot.
