# ADR-0367: Registered Stream Membership Reconciliation

Status: accepted

Date: 2026-10-22

## Context

Registered stream synchronization validates ontology instance-membership facts,
but the stream premise index currently records only rule-witness premises. A
membership retraction can therefore invalidate claims that do not contain that
membership edge as a rule witness. ADR-0364 currently rejects such a successor
and requires a complete registered rebuild.

## Decision

1. Keep ordinary registered successors delta-seeded and use the existing exact
   premise index for their closure/discovery work.
2. When a canonical successor closes one or more registered instance-membership
   facts, evaluate the complete bounded registered rule set against that
   successor's selected current graph. Do not replay earlier canonical history.
3. In the same downstream SQLite transaction, close the old active stream claim
   set with the exact membership closure versions as reasons, add the complete
   target-generation result, and advance the checkpoint/active root atomically.
4. Enforce existing claim, posting, evidence, output-byte, premise, and page
   budgets during reconciliation. If admission or evaluation fails, publish no
   stream effect. Penelope retains its existing registered stream request and
   retry/recovery contract.
5. Do not change Claim DTOs, identities, journal payloads, or schema. This is a
   conservative full-current-selection reconciliation only on ontology typing
   edits; a future exact validation-premise index may make it incremental.

## Consequences

Membership retraction/restoration no longer forces users to create a new stream
baseline, and stale registered claims cannot remain active. The exceptional
path is proportional to the bounded current-selection evaluation and active
claim set; ordinary updates retain delta-seeded behavior. Exceeding any bound
leaves the prior stream head intact.

## Verification

Membership retraction and restoration match complete registered evaluation;
exhausted evidence admission leaves the previous checkpoint intact. A
Penelope-prepared registered request recovers the SQL-published reconciliation
idempotently. The inference SQLite suite (24 tests), feature-enabled Penelope
suite (38 tests), strict all-target Clippy, formatting, migration registries,
architecture checks, and full contributor CI (161.84 seconds) pass.

## References

- [ADR-0364](0364-registered-inference-stream-successors.md)
- [ADR-0363](0363-penelope-registered-inference-rebuilds.md)
- [SyntaxMesh v5 source of truth §78.5](../syntaxmesh-todo-extensible-opensource-v5.md#785-ontology-versioning)
