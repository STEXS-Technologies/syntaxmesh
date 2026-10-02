# ADR 0014: Engine-owned extension fact ingestion

- Status: Superseded by ADR 0015 for runtime-observation handling; extension-fact ingestion remains accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

The extension SDK defines versioned manifests, namespaces, capabilities, and
validated fact batches, but the full engine has no path for an extension to
publish those facts. Direct access to `GraphStore` would bypass Penelope's
durable workflow, accepted-generation coordination, optional StateChronicle
verification, and the engine's responsibility to validate external input.

## Decision

Add an engine application method that accepts a validated `FactBatch`, an
idempotent run ID, and a caller-selected next generation. It rejects batches
containing runtime observations until the distinct runtime-evidence
publication contract exists. For supported batches, it publishes the
namespaced facts and producer provenance as one atomic `GraphDelta` through
Penelope and applies the same optional StateChronicle generation verification
as source indexing. Extensions receive no store handle and cannot mutate
canonical state directly.

## Consequences

- An extension can add canonical namespaced facts through public SDK and engine
  APIs, with durability and recovery owned by the engine.
- Extension ingestion is additive in this first slice; removal/replacement
  lifecycle, extension registry persistence, capability negotiation across
  processes, and failure isolation remain open.
- Runtime observations are not accepted by this method; they require a
  separate persisted evidence layer and must not be converted into static
  facts.
- The caller remains responsible for deterministic generation and run IDs;
  the engine validates the batch and applies optimistic base-generation
  publication semantics.

## Alternatives considered

- Let extensions call `GraphStore::apply_delta`: rejected because it bypasses
  engine workflow and verification semantics.
- Merge extension facts into source extraction: rejected because it couples
  unrelated producers and makes source re-indexing own extension lifecycles.
- Silently ignore the runtime observations in `FactBatch`: rejected because
  accepted data must never disappear without a diagnostic.

## Migration and compatibility

This adds a method and error variant to the Rust engine API but changes no
canonical model, store schema, or export schema. Existing source indexing is
unchanged. Consumers matching `EngineError` exhaustively must handle the new
extension-ingestion failures.
