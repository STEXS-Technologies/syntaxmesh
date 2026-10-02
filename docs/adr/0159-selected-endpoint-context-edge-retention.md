# ADR-0159: Retain only usable context edges

- Status: accepted
- Date: 2026-09-30

## Decision

Apply the context packer's existing selected-endpoint filter during expansion,
after attempting admission of the opposite node. Retain an edge only when both
endpoints are selected. The node set grows monotonically to a fixed candidate
limit; a node refused because the limit is reached cannot be admitted later.
An absent node remains absent in the pinned generation. Therefore an edge
discarded here cannot become a usable graph-path candidate on a later hop.

Reuse the existing deterministic traversal, stable-ID edge ordering, store
ports, and BTreeMap deduplication. No public contract, warning, migration, or
dependency changes. Apply this to both current and historical compilation.

## Limits and verification

This removes retained edges to discarded endpoints, not adjacency scanning or
the temporary adjacency vector. Parallel edges among selected nodes may still
be large. Do not claim bounded total memory or constant-time queries.

Verify high-degree candidate saturation retains no unusable edges, selected
parallel/incoming/self edges survive with deduplication, and existing context
packs remain equivalent across backends, generations, and restart.

The 1,001-parallel-edge fixture plus incoming/self edges passes for a saturated
one-node selection, admission of the second node, and preselected endpoints.
Its retained map equals the original final selected-endpoint filter, with no
extra node reads after saturation. Full `cargo make ci` passes, including strict
Clippy, current/historical four-backend context conformance, restart, and MCP.
