# ADR-0155: Indexed historical evidence reads

- Status: accepted
- Date: 2026-09-30

## Decision

Add generation-pinned `historical_file` and `historical_provenance` point reads
to the existing GraphStore port. Expose the same point reads on generation-pinned
Query, following historical_node. Reuse the durable adapters' historical-node
validity-index query and codecs; do not introduce a schema, migration, cache,
workflow, or new dependency. Unknown generations fail; absent facts return None.
Validity intervals remain inclusive at their start and exclusive at their end.

Reference defaults follow the existing historical-node snapshot fallback.
SQLite and Turso override them with indexed identity/interval reads, never replay
or full-snapshot loading. These ports are prerequisites, not a claim that the
ordinary context compiler or MCP supports historical context. Temporal search
and context routing remain required by ADR-0154.

## Verification

Cross-backend fixtures cover original/replaced/deleted file versions, retained
provenance, missing identities, unknown generations, and durable restart.
The replacement fixture now changes producer version and evidence class under
the same provenance identity; pinned Query reads must retain the original and
return the revised record only from the replacement onward. Full historical
context remains additional implementation work.
