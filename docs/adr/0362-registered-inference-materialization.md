# ADR-0362: Registered Inference Materialization

Status: accepted

Date: 2026-10-22

## Context

Schema-backed inference currently validates retained fact typing through the
pure Query API, while the downstream SQLite materializer rebuilds only with
unregistered rules. Binding ontology fingerprints into identities without
running the registry validator would allow a durable claim to be published
without the declared ontology constraints being checked.

## Decision

1. Add an explicit `InferenceMaterializer::rebuild_registered` operation that
   accepts a caller-owned `OntologyRegistry`, binds the rule set, evaluates the
   exact retained graph generation through registered Query validation, and
   atomically publishes the resulting claims using the bound rules for
   operation/configuration identity.
2. Keep ontology definitions and registry state outside canonical graph
   storage. This operation does not migrate or persist the registry; callers
   must supply the same definitions to reproduce the same semantic operation.
3. Keep the existing unregistered rebuild behavior and request/journal wire
   layouts unchanged. Penelope-integrated registered setup/recovery and durable
   ontology catalog history require a separate versioned workflow/storage
   decision before they are exposed as full-engine capabilities.

## Consequences

Low-level downstream SQLite consumers can materialize schema-validated claims
without replaying or mutating canonical graph history. Ontology-invalid facts
fail before publication, and same-version schema edits produce distinct
materialization identities. The current Penelope setup/stream workflow remains
unregistered; this ADR does not claim registered crash recovery or historical
ontology catalog retrieval.

## Verification

Tests compare registered durable output to registered Query output, verify
schema edits change operation identity, reject invalid fact typing without
publishing a watermark, and retain/reopen valid bound claims.

## References

- [ADR-0346](0346-durable-inference-materialization-boundary.md)
- [ADR-0360](0360-explicit-ontology-instance-membership.md)
- [ADR-0361](0361-ontology-fingerprints-in-inference-identity.md)
- [SyntaxMesh v5 source of truth §104](../syntaxmesh-todo-extensible-opensource-v5.md)
