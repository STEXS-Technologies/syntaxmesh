# ADR 0015: Persist runtime observations as separate evidence nodes

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

The public runtime protocol and extension `FactBatch` can carry observations,
but the engine previously rejected them because no durable, separate evidence
representation existed. Dropping or translating an observation into a static
call/dependency edge would violate the product rule that runtime evidence
augments, but never rewrites, static facts. Adding an observations column to
every backend would duplicate the existing atomic graph-generation mechanism
and require coordinated database migrations.

## Decision

Represent each accepted observation as a distinct canonical `RuntimeObservation`
node. Its identity is derived from the exact serialized versioned observation
envelope; its namespaced extension payload stores those bytes unchanged; and
its provenance uses `EvidenceClass::RuntimeObserved` with the declared producer
namespace/version. It does not create edges to static nodes because protocol
subject/object identifiers are opaque and no authoritative mapping contract
exists yet. The node is published alongside the extension batch in the same
Penelope-backed atomic graph generation. Append the new `NodeKind` variant so
existing bincode enum ordinals do not move.

## Consequences

- A full-engine host can durably ingest observations using the public protocol
  and query/export them as explicit runtime evidence.
- Static facts, IDs, and edges are not overwritten or inferred from opaque
  runtime identifiers.
- Cross-layer subject/object resolution, observation retention/deletion,
  event ordering policies, and dedicated observation query APIs remain open.
- Backends require no new table; they persist the typed node and provenance
  through the existing generation transaction and graph-root validation.
- Serialized observation bytes use the public protocol's serde JSON encoding;
  the protocol schema version is stored in the extension payload.

## Alternatives considered

- Persist observations in a second non-atomic table: rejected because graph
  generation publication and runtime evidence could diverge across crashes.
- Convert subject/relation/object directly to static graph edges: rejected
  because opaque producer identifiers are not canonical node identities and
  runtime behavior must not silently become static truth.
- Silently discard observations in `FactBatch`: rejected because data loss
  would be invisible to the producer.

## Migration and compatibility

This appends one `NodeKind` variant and changes no table schema or
`GraphDelta` layout. Old bincode variant ordinals remain stable. Consumers
matching `NodeKind` exhaustively must handle `RuntimeObservation`; old readers
may not understand exported payloads that contain the new variant. The engine
extension ingestion API now accepts runtime observations and turns them into
separate evidence nodes rather than returning the temporary unsupported error.
