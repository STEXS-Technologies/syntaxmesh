# ADR 0375: Bounded ontology-version compatibility

Status: accepted

Date: 2026-10-22

## Context

Ontology definitions are registered and fingerprinted per namespace/version,
and those fingerprints bind inference-rule identity. Callers can therefore
distinguish schema revisions, but cannot determine whether a newer definition
preserves the constraints and vocabulary of an older one. The v2 product
requires ontology versioning and independent invalidation without pretending
that changed semantics are interchangeable.

## Decision

Add a pure `OntologyRegistry` comparison for an ordered `(namespace,
from_version, to_version)` pair. It compares complete registered definitions
under a caller-supplied work budget and returns the first deterministic
schema-level incompatibility, if any. Compatibility means the target retains
all source concepts and relations, preserves the instance relation, and does
not narrow any declared relation domain or range. Added concepts/relations and
widened signatures are backward-compatible. An empty domain/range means
unrestricted and is treated as the full set when checking widening.

The result is advisory schema compatibility only. It does not assert semantic
equivalence, migrate facts, rebind rules, or reuse claims across ontology
fingerprints. Version-only registries without definitions cannot answer the
comparison. No serialized definition or persistence format changes.

## Consequences

- Extensions can check whether consumers of an older schema remain structurally
  valid under a newer schema, with explicit failure for unknown versions and
  exhausted comparison work.
- Exact version fingerprints remain part of inference identity even when the
  schema comparison reports backward compatibility; historical derivations
  retain their original ontology versions.
- Future durable ontology catalogs can store these definitions and reuse the
  same pure comparison rather than inventing database-specific semantics.

## Verification

Test compatible additions and widening, removed concepts/relations, narrowed
domains/ranges, instance-relation changes, unknown/version-only registries,
work-budget exhaustion, and deterministic outcomes independent of declaration
order.

## References

- SyntaxMesh source of truth v5, sections 78.5, 79, 316.
- `crates/syntaxmesh-inference/src/ontology.rs`.
- ADRs 0337 and 0358–0361.
