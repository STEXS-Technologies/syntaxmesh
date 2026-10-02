# ADR-0265: Separate source preparation from publication

Status: accepted, 2026-10-01.

## Context

Resolver filesystem coverage is discovered while preparing source facts. Hosts
need a validation boundary before accepting those facts.

## Decision

Expose `SyntaxMeshEngine::prepare_source_delta`, delegating to the existing
Indexer after workflow recovery. Return the existing runtime-neutral GraphDelta;
introduce no DTO, workflow, schema, or filesystem dependency. Recovery can accept
earlier pending operations, but preparation does not publish the new delta.

Hosts can discard it or use `publish_prepared_with_lineage` with empty lineage.
Scope validation, expected-base checks, Penelope and StateChronicle remain on
the existing publication path. `index` delegates to preparation and retains its
behavior. Hosts retain exclusive writer ownership across both operations.

## Consequences

This enables validation without duplicating indexing or workflow orchestration.
Automatic invalidation still requires host integration, dependency discovery
convergence and generation-bound coverage. Hashing newly discovered inputs only
after preparation does not prove those bytes produced the prepared facts. This
API does not provide an atomic filesystem snapshot.
