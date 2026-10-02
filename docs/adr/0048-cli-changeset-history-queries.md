# ADR-0048: Expose ChangeSet history through the local CLI

- Status: accepted
- Date: 2026-09-28

## Context

The accepted ChangeSet contract in [ADR-0041](0041-explicit-change-set-membership.md)
provides a declaration lookup at a pinned generation and bounded, cursor-paged
event membership reads. The engine/query APIs and durable indexes exist, but
the local File/Turso CLI has no way to inspect that evidence. A user therefore
cannot exercise this part of the history graph without embedding Rust APIs.

The Change Engine owns mutation workflows. This ADR does not add authoring,
assignment, inference, or repository mutation to the SyntaxMesh CLI; it only
surfaces already accepted facts through the existing query service.

## Decision

1. Add File and Turso CLI commands for `ChangeSetAt` and
   `EventsForChangeSet`.
2. Require an explicit generation for each query. Event paging accepts an
   optional prior event-generation cursor, while the result footer supplies the
   continuation cursor and retains its snapshot binding.
3. Emit the existing versioned temporal NDJSON records unchanged. Do not add a
   parallel CLI DTO, scan deltas, or infer membership.
4. Keep authoring and assignment outside the CLI. A future authoring surface
   requires a separate decision and must preserve the Change Engine boundary.

## Consequences

- Users can inspect ChangeSet declarations and page explicit event membership
  from either supported CLI store.
- The CLI remains a thin host over `SyntaxMeshEngine::query`; durable stores
  retain their indexed, fixed-generation read path.
- This closes read access only. Product-facing authoring remains open and is
  not implied by these commands.
