# ADR-0158: Edge-only temporal context expansion

- Status: accepted
- Date: 2026-09-30

## Decision

Reuse the existing generation-scoped `historical_incident_edges` store port for
context expansion. Factor the query layer's cursor and page validation into an
internal edge-only helper shared with `historical_neighbors`. The public neighbor
query still hydrates and validates every returned neighbor. Context consumes edge
pages directly and loads node payloads only when its existing deterministic
candidate selection admits them.

This applies the existing store/query separation instead of adding another
index, cache, schema, workflow, dependency, or public contract. Preserve edge
ordering, incoming/outgoing coverage, candidate limits, token accounting, and
context-pack content for valid retained graphs. Publication/backend integrity
remains responsible for complete endpoint validity; context is not a substitute
for scanning discarded neighbor payloads for corruption.

## Limits and verification

The change removes discarded neighbor hydration, not the incident-edge scan or
its memory cost. High-degree edge work and lexical scans remain open scaling
gates; no constant-time or end-to-end latency claim is justified.

Verify edge-only paging without neighbor reads, multi-page ordering, cursor
validation, and unchanged public neighbor hydration. Run existing cross-backend
historical context and MCP protocol fixtures and the full contributor gate.

The 1,001-edge port fixture passes with two outgoing pages, two incoming pages,
one empty incoming page, and zero neighbor payload reads. A public neighbor call
still attempts hydration; invalid cursor/limit calls never reach the store.
Full `cargo make ci` passes, including strict Clippy, four-backend historical
context/restart conformance, MCP server and stdio tests, and architecture guards.
