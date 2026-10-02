# ADR 0007: Bounded, explicit subgraph export

- Status: accepted
- Date: 2026-09-27

## Context

The query plan calls for bounded NDJSON subgraphs suitable for embedding and context retrieval. The current exporter always serializes a complete generation and gives callers no indication that a bounded result was truncated. A limit must be deterministic and visible to consumers rather than silently dropping facts.

## Decision

Add `Query::export_subgraph` with non-empty seed IDs and explicit hop, node, and edge limits. Traversal is weakly connected (both incoming and outgoing relations), breadth-first, and stable under seed ordering. Seed IDs must exist, all limits must be non-zero, and the unique seed count must fit the node limit. The returned graph contains only selected nodes, edges whose endpoints are both selected, and their referenced provenance records. When node or edge caps omit eligible facts, the footer sets `truncated`.

Add the `truncated` boolean to the versioned NDJSON footer and bump the graph export schema version to 2. Complete-generation exports set it to false. The output is bounded by record counts; byte-size limiting remains a future contract.

## Consequences

- Consumers can distinguish a complete bounded neighborhood from one cut by limits.
- Existing schema-v1 readers must continue to support legacy records; schema-v2 consumers must read the new footer field.
- Traversal still uses the generation-tagged graph projection. Avoiding a complete in-memory projection per query remains a separate optimization gate.

## Verification

Test deterministic output independent of seed order, hop/node/edge caps, truncation signaling, unknown seeds, and generation binding. Round-trip every emitted v2 NDJSON record.
