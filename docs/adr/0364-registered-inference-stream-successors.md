# ADR-0364: Registered Inference Stream Successors

Status: accepted

Date: 2026-10-22

## Context

Registered complete rebuilds now persist and recover their ontology definitions,
but stream successors initially lacked registered delta discovery. Registered
validation also depends on temporal instance-membership facts, while the stream
premise index tracks rule-witness premises; membership changes therefore need
reconciliation beyond ordinary premise invalidation.

## Decision

1. Add registered seeded discovery that validates the selected schema facts and
   discovered claims while retaining the existing delta-seeded rule traversal.
2. Add registered SQLite synchronization and a separate versioned Penelope stream
   journal. Persist canonical ontology definitions and their admission bounds in
   each request; existing stream request payloads and keys stay unchanged.
3. Reconcile an ontology instance-membership retraction through a bounded full
   evaluation of the target selection and atomically replace the active claim
   set, as specified by [ADR-0367](0367-registered-stream-membership-reconciliation.md).
   Ordinary successors remain delta-seeded.
4. Keep ordinary unregistered streams unchanged. No migration is added by this
   ADR because the accepted operation uses existing stream and journal tables.

## Consequences

New registered claims are checked against all active facts at the target
generation, while witness discovery remains restricted to opened exact premise
versions. Ordinary relation closures continue to use the existing dependency
index. Membership retractions are reconciled without replaying earlier canonical
history; any exceeded bound preserves the previous downstream head.

## Implemented evidence

`Query::infer_discovered_bound_rules` validates the full active schema fact set
at the target generation, then uses the existing exact-opened-version seed walk
and validates its claims. `InferenceMaterializer::sync_stream_registered`
publishes that result through the existing atomic successor transaction. Exact
closed membership versions are resolved through the store's version index; a
membership retraction triggers complete target-selection reconciliation.
Penelope carries this operation through a new sequence-prefixed journal request that persists
canonical definitions and bounds. Existing unregistered request types and
prefixes are unchanged.

The Query suite (66 tests), SQLite inference suite (24 tests), and feature-enabled
Penelope suite (38 tests) pass, including registered delta parity, valid
successor publication, persisted definition replay, membership retraction and
restoration, and prepared-request recovery. Strict all-target Clippy passes.
Full host command delivery and full contributor CI remain open.

## References

- [ADR-0347](0347-retained-inference-premise-closure-lifecycle.md)
- [ADR-0351](0351-penelope-inference-stream-synchronization.md)
- [ADR-0360](0360-explicit-ontology-instance-membership.md)
- [ADR-0363](0363-penelope-registered-inference-rebuilds.md)
- [SyntaxMesh v5 source of truth §78.5](../syntaxmesh-todo-extensible-opensource-v5.md#785-ontology-versioning)
