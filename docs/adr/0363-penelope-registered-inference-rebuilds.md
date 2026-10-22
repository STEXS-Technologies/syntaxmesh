# ADR-0363: Penelope Registered Inference Rebuilds

Status: accepted

Date: 2026-10-22

## Context

The downstream SQLite materializer can rebuild registered inference, but the
Penelope coordinator only journals unregistered rebuild requests. Adding ontology
definitions to the existing bincode request would risk decoding existing durable
records and would leave recovery dependent on caller-owned in-memory state.

## Decision

1. Add an explicit Penelope registered-rebuild operation and recovery scan using a
   new versioned journal prefix and request type. Existing unregistered request
   layouts, keys, and recovery behavior remain unchanged.
2. Persist the complete bounded ontology definitions and registry/binding
   admission bounds in each registered request. Recovery reconstructs and
   validates the registry from that request before executing or validating the
   downstream effect.
3. Bind rules once and use the resulting rules for operation identity, registered
   Query validation, and materialization. The existing SQLite transaction remains
   the atomic publication boundary.
4. Keep ontology catalog history, automatic cross-version compatibility, and
   registered incremental stream synchronization out of this change. This API is
   an explicit complete rebuild, not a claim that ontology-aware streaming is
   complete.

## Consequences

Prepared registered rebuilds can recover without resubmitting their ontology
definitions, including after a crash before or after SQLite publication. Existing
journal records remain readable. The journal's existing byte admission limit
continues to cap persisted requests. Registered stream successor processing
remains a separate required step before an end-to-end ontology-aware stream can
be offered.

## Verification

Tests must cover restart recovery before and after SQLite publication, schema
rejection without partial publication, request substitution rejection, and
unchanged unregistered journal encoding.

## References

- [ADR-0346](0346-durable-inference-materialization-boundary.md)
- [ADR-0361](0361-ontology-fingerprints-in-inference-identity.md)
- [ADR-0362](0362-registered-inference-materialization.md)
- [SyntaxMesh v5 source of truth §104](../syntaxmesh-todo-extensible-opensource-v5.md)
