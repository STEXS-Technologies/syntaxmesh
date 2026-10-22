# ADR-0360: Explicit Ontology Instance Membership

Status: accepted

Date: 2026-10-22

## Context

ADR-0359 validates relation signature references and target-range compatibility,
but cannot verify whether retained relation endpoints actually have the
required concepts. `NodeKind` is structural syntax, not domain typing. The
ontology schema therefore needs an explicit, versioned way to identify
`INSTANCE_OF` evidence before domain/range checks can be meaningful.

## Decision

Advance ontology definitions to schema version 3 and add an explicit
`instance_relation` name. It must name one relation declared by that ontology;
its object range must equal that ontology's declared concept set. This relation
is the only source of ontology membership for schema validation; node labels or
structural kinds never imply a concept.

For schema-backed Query evaluation, collect memberships from active retained
facts using the exact namespace/version relation. Then validate every active
fact using a registered constrained relation: its subject must have at least
one declared domain concept and its object must have at least one declared
range concept. Instance assertions themselves require a declared target
concept and are not recursively range-typed. Constrained derived output
relations validate the resulting claim endpoints by the same membership map.
Every bound concept ID must also be present in the selected graph. Empty
endpoint constraints remain unconstrained for ordinary relations. All scanning
and membership comparisons are bounded by the existing fact and edge-visit
budgets; budget exhaustion rejects the whole result.

The selected rules must bind one consistent version per namespace. Facts in
unbound namespaces and ordinary relations with no endpoint constraints are not
type-checked by this validator.
This remains an optional read/verification path; it adds no canonical mutations,
storage, transport or host dependency.

## Consequences

Schema-backed inference now fails closed when retained facts lack declared
domain/range evidence or use an undeclared instance target. Applications must
identify the membership relation explicitly. The initial implementation
validates facts read for on-demand Query inference; durable materializers and
diagnostic explanation APIs must integrate the same validator before claiming
ontology conformance.

## Verification

Pure fixtures cover valid membership, missing subject/object concepts, invalid
instance targets and edge-visit exhaustion. Shared Query fixtures verify the
same behavior on InMemory, File, SQLite and Turso retained selections.

## References

- [ADR-0358](0358-bounded-ontology-schema-admission.md)
- [ADR-0359](0359-bounded-ontology-relation-signatures.md)
- [SyntaxMesh v5 source of truth §78.3](../syntaxmesh-todo-extensible-opensource-v5.md)
- [SyntaxMesh v5 source of truth §79.1](../syntaxmesh-todo-extensible-opensource-v5.md)
- [V0 architecture and implementation plan](../V0_ARCHITECTURE_PLAN.md)
