# ADR 0245: Trait-only implementation resolution

Status: accepted

## Decision

Reuse the existing reference/name-index pipeline for `RelationKind::Implements`,
but match only trait declarations. Keep trait declarations eligible for indexer
candidate discovery and edit invalidation without making them call targets.
All existing exact normalization, ambiguity, qualified-miss preservation,
occurrence evidence, and edge identity rules apply. Advance the shared static
resolver revision to invalidate unchanged-source host planning.

This is a resolver capability for source producers, not Rust `impl` extraction.
It adds no DTO, database schema, dependency, runtime, or private lookup algorithm.

## Limits

This does not emit Rust implementation facts or infer implementing types, trait
dispatch, import aliases, lexical scope, visibility, or compiler semantics.
Name-based matching remains conservative but not compiler resolution. Historical
graphs are not rewritten.

## Verification

All 11 resolver and nine source-host tests pass. Resolver fixtures require
trait-only matching, non-callable traits, explicit ambiguity, and qualified misses.
A synthetic SDK producer indexer fixture verifies that changing a same-named
function into a trait resolves an unchanged implementation occurrence, changing
it back retracts that edge, and the resolved old generation retains its trait
and edge. The producer is explicitly synthetic, not proof of Rust `impl`
extraction. Full `cargo make ci` passes in 234.48 seconds: architecture boundaries,
extension isolation, migration registries, strict Clippy without new allowances,
workspace tests, all 16 backend-conformance scenarios, and documentation
generation. Existing allowed dependency policy warnings remain unchanged.
