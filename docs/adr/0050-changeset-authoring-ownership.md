# ADR-0050: ChangeSet authoring belongs to the Change Engine

- Status: accepted
- Date: 2026-09-28

## Context

SyntaxMesh stores explicit, provenance-backed `ChangeSet` declarations and
event membership as part of its temporal history graph. The Change Engine is a
separate project that owns change-intent lifecycle and mutation workflows.
SyntaxMesh already exposes `SyntaxMeshEngine::publish_prepared_with_lineage`
for a caller such as the Change Engine to submit a prepared graph transition
with explicit lineage. The engine validates scope, durably publishes through
Penelope, and makes the accepted evidence queryable; it does not plan or author
the change workflow.

The v0 plan currently lists “product-facing ChangeSet authoring access” as
remaining Gate 1 work. That wording risks pulling a Change Engine workflow into
the SyntaxMesh product boundary even though read-only ChangeSet queries and the
prepared-publication contract already exist.

## Decision

1. SyntaxMesh owns the `ChangeSet`/membership data model, validation, atomic
   persistence with accepted generations, provenance, and pinned historical
   queries.
2. The Change Engine owns creation and lifecycle of user-facing ChangeSet
   authoring workflows. It submits prepared graph and lineage evidence through
   SyntaxMesh's existing prepared-publication boundary; it does not write store
   tables directly.
3. SyntaxMesh CLI remains read-only for ChangeSet history. No authoring command,
   intent workflow, or mutation policy is added to SyntaxMesh v0.
4. Product-facing ChangeSet authoring is not a SyntaxMesh Gate 1 exit blocker.
   Integration behavior belongs to the Change Engine project and requires its
   own cross-project decision and tests when that project is in scope.

## Consequences

- SyntaxMesh Gate 1 remains focused on canonical persistence and temporal
  history, not a second mutation orchestrator.
- The prepared publication API is the only accepted route for a future
  Change Engine to attach explicit ChangeSet lineage to an accepted generation.
- SyntaxMesh still needs its documented caller contract and conformance tests;
  a user-facing authoring workflow is deferred to the Change Engine.
