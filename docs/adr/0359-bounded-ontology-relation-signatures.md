# ADR-0359: Bounded Ontology Relation Signatures

Status: accepted

Date: 2026-10-22

## Context

ADR-0358 admits namespace/version-owned concept IDs and relation names, but a
relation declaration carries no subject-domain or object-range information.
The v2 specification explicitly calls for typed relation schemas that support
validation and inference safety. Public ontology definitions are not yet
persisted, so this contract can be extended before a durable format exists.

## Decision

Advance the pure ontology-definition schema to version 2. Replace relation-name
strings with `OntologyRelationDefinition` values containing the exact local
name and bounded subject-domain/object-range concept ID sets. Empty domain or
range sets mean that side is unconstrained. Every listed concept must belong to
the same ontology definition; identifiers and entries remain subject to the
existing caller-provided byte and entry budgets.

Schema-backed rule admission continues to require exact namespace/version
ownership. If a rule's declaration relation or derived output relation declares
an object range, that range must include the rule's target concept. Built-in
relations remain unaffected. This prevents internally inconsistent rule/schema
bindings without changing the canonical graph or introducing storage/host
dependencies.

This slice does not yet type-check every retained edge's subject and object
against asserted concept memberships, enforce domain constraints where a rule
does not declare its source concept, emit diagnostics, or persist ontology
schemas. Those require an explicit instance-membership model and broader query
semantics; they remain v2 work rather than being guessed from node syntax.

## Consequences

Callers must construct version-2 relation declarations. Invalid references,
duplicates and over-budget signature sets fail closed at registration. Existing
version-pair registration is unchanged. The schema can later support full
fact-level validation and planning without placing domain semantics in
`syntaxmesh-core`.

## Verification

Pure inference fixtures cover signatures, declared concept references,
duplicate and empty identifiers, rule-target range compatibility and combined
entry/byte bounds. Schema-backed Query fixtures continue to exercise all four
store implementations.

## References

- [ADR-0358](0358-bounded-ontology-schema-admission.md)
- [SyntaxMesh v5 source of truth §78.3](../syntaxmesh-todo-extensible-opensource-v5.md)
- [V0 architecture and implementation plan](../V0_ARCHITECTURE_PLAN.md)
