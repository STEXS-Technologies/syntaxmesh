# ADR-0358: Bounded Ontology Schema Admission

Status: accepted

Date: 2026-10-22

## Context

ADR-0337 adds an explicit namespace/version registry, but deliberately does not
verify that a selected inference rule refers to concepts or relations declared
by that ontology. SyntaxMesh's v2 product specification calls for a separate
ontology plane with namespace-owned concepts and relations, while preserving
the fact plane and keeping ontology optional for ordinary indexing.

## Decision

Add a pure, bounded version-1 `OntologyDefinition` to
`syntaxmesh-inference`. A definition declares an ontology namespace/version,
the stable graph node IDs representing its concepts, and its exact local
relation names. `OntologyRegistry::from_definitions` admits definitions only
when namespace/version and concept/relation identifiers are non-empty and
unique, definition identities are unique, and the combined definition/concept/
relation entry count and encoded input bytes fit caller-provided bounds.

When a registry built from definitions validates a rule set, every external
relation used for the rule's path, join, declaration, or output must be declared
by the exact namespace/version explicitly bound on that rule. Each rule's
declaration target concept must be declared by one of its bound ontologies.
Built-in core relations need no ontology declaration. The existing version-pair
constructor remains available as the weaker ADR-0337 version-existence check.
The existing Query registered path performs schema admission before reading
retained facts.

The registry is supplied by the caller for each evaluation. No ontology file
loader, database persistence, migration, source authentication, schema
compatibility inference, relation domain/range checking, or automatic fallback
is added. Inference remains opt-in and never modifies canonical facts. This
provides a useful v2 schema-admission slice without placing ontology policy in
`syntaxmesh-core` or coupling it to storage/hosts.

## Consequences

Rule typos and undeclared concepts fail before graph/history access when callers
choose schema-backed registration. Existing callers using only explicit version
pairs retain their current behavior. Applications own the provenance and
reproducibility of supplied definitions; later work may add a durable registry
after ontology identity, compatibility, and migration semantics are specified.

## Verification

Pure fixtures cover exact concept/relation admission, wrong or missing
namespace/version bindings, undeclared relation/concept rejection, duplicate
and empty declarations, and independent count/byte bounds. The Query method
validates the registry before calling ordinary retained-fact evaluation; shared
Query fixtures verify schema-backed success and rejection on InMemory, File,
SQLite and Turso adapters, including the SQLite/Turso retained-history reopen
integration test.

## References

- [ADR-0336](0336-rule-set-ontology-version-consistency.md)
- [ADR-0337](0337-explicit-ontology-version-registry.md)
- [SyntaxMesh v5 source of truth §78](../syntaxmesh-todo-extensible-opensource-v5.md)
- [V0 architecture and implementation plan](../V0_ARCHITECTURE_PLAN.md)
