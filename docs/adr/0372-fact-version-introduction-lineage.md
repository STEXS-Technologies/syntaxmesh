# ADR-0372: Fact-Version Introduction Lineage

Status: accepted

Date: 2026-10-22

## Context

The v5 temporal stages require `Origin` and `IntroducedBy` queries over
retained history. SyntaxMesh already retains exact fact-version validity
coordinates and one indexed `ChangeEvent` per accepted generation, but clients
must currently join these records themselves and can accidentally imply
cross-fact causality.

## Decision

1. Add a runtime-neutral Query operation that accepts an exact
   `FactVersionRef` and resolves the accepted `ChangeEvent` at its
   `valid_from` generation.
2. Return a dedicated temporal record only when that event names the exact fact
   as `Added` or `Updated`. `Removed`, no-op events, missing events (including
   imported history anchors without an event), and events that do not match the
   requested version produce `None`. An unknown generation remains a store
   error rather than being confused with an anchor lacking an event.
3. Label the relation as version-introduction lineage. It says which accepted
   transition opened this canonical fact-version interval; it does not claim
   that the transition caused any other fact, consequence, or later event.
4. Verify the exact interval exists through `GraphStore::fact_version_exists`
   before reading its `change_event`. Durable stores use the existing
   fact-identity/start index; no database state or migration is added.
5. Advance the temporal interchange schema version because a new record kind
   is added. Keep the core and API model independent of storage/workflows.

## Consequences

This provides one exact, auditable temporal join primitive while retaining the
distinction between transition membership and causality. The current
`GraphStore::change_event` implementation is indexed in durable stores; the
reference implementation may scan its retained history. Broader semantic
origin/blame, change-to-change causality, and `ResolvedBy` remain separate work.

## Verification

Test an initial addition, an update, removal/reintroduction, a no-op event,
missing event coordinates, exact record serialization, and retained lookup
after durable reopen through the existing store conformance gate.

## References

- [v5 temporal implementation stages](../syntaxmesh-todo-extensible-opensource-v5.md#316-v5-implementation-stages)
- [v0–v5 implementation plan](../V0_ARCHITECTURE_PLAN.md)
