# ADR-0089: Bound historical-neighborhood output bytes

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0088 bounds the number of selected nodes, edges, and scanned incidence
entries. Shardline's bounded dataset-query implementation adds an independent
result-byte cap because a small row count can still contain unusually large
values. SyntaxMesh edge extensions and node extension payloads are open-world
data, so item-count limits alone do not bound an NDJSON response.

## Decision

1. Historical-neighborhood queries and their stream accept a positive
`max-result-bytes` budget capped at 16 MiB, matching Shardline's established
result-limit scale. The budget includes every serialized node/edge item line,
its newline, the final summary footer, and its newline.
2. Reserve enough space for the largest possible footer before selecting any
non-seed output. All seeds are mandatory anchors; if the requested budget
cannot hold the seed records plus the footer, reject the request with a typed
budget error rather than returning an anchorless neighborhood.
3. A candidate node and the edge that discovers it are admitted together, so
the result never returns an edge whose selected neighbor node was omitted by
the byte budget. Existing-node edges are admitted only when their whole line
fits. Once eligible output is omitted due to the byte cap, set `truncated`.
4. Count JSON bytes through a capped writer rather than first allocating an
unbounded serialized string. The stream footer reports serialized item bytes
in addition to counts, scanned incidences, and truncation.

## Consequences

- Scan work and output bytes are independent explicit bounds, following
Shardline's `MAX_QUERY_SCAN_ROWS` and `MAX_RESULT_BYTES` separation.
- The output byte budget is not a cap on bytes fetched/decoded for one
individual persisted fact; existing store row payload limits remain a
separate concern.
- The limit affects only historical-neighborhood traversal, not complete graph
exports or other temporal query families.

## Verification

Test exact byte accounting including newlines/footer, mandatory-seed failure,
oversized-item omission with correct truncation, endpoint closure of returned
edges, invalid/oversized budgets, and equal File/Turso streams.
