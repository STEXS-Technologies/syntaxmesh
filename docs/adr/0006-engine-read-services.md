# ADR 0006: Route host reads through engine application services

- Status: accepted
- Date: 2026-09-27

## Context

The architecture plan requires embedded and standalone hosts to share application semantics. The current CLI opens a graph store and constructs query services itself, while the engine exposes only a generation-scoped query view and no status result. This duplicates composition rules in each host and makes the CLI a privileged store client.

## Decision

Expose a generation-scoped node lookup on `Query` and a typed current-scope status service on `SyntaxMeshEngine`. Refactor file and Turso CLI reads to construct the same engine used for indexing, then call its query/status API. Status includes the accepted manifest and canonical file, node, edge, and provenance counts. Keep host formatting and argument parsing in the CLI; do not expose the engine's store.

## Consequences

- Embedded callers and the CLI use one read path and one generation boundary.
- Adding fields to the status DTO or changing query behavior is an application API decision and requires compatibility tests.
- Existing store-level APIs remain available for backend adapters, indexing, and conformance.
- Bounded subgraph export remains a separate query-surface task; full-generation NDJSON export is unchanged.

## Verification

Test current/empty status behavior and generation-scoped node lookup; compare CLI and embedded results on the same fixture; run file and Turso smoke paths.
