# ADR-0179: HTTP bounded historical neighborhoods

- Status: accepted
- Date: 2026-09-30

## Decision

Reuse the existing generation-scoped weak-neighborhood BFS for GET
`/api/v1/nodes/{id}/neighborhood`. Do not add a host traversal, replay, full graph
load, migration, workflow, or NDJSON service handoff. The existing algorithm
currently budgets NDJSON export records; introduce a pure caller-supplied output
sizer and grouped limits so HTTP can budget its own JSON shape. Preserve the
existing method and CLI/export behavior with an NDJSON export sizer. The new
query contract imports no host/runtime dependency and also supports future
protocols without changing graph semantics.

The HTTP route accepts optional `generation`, `max_hops` (1–8, default 2),
`max_nodes`/`max_edges` (1–256, default 64/128), `max_scanned_edges`
(1–4096, default 1024), and `max_result_bytes` (1–1,048,576, default 262,144).
Reject unknown/malformed/oversized limits with 400. The path supplies one seed;
missing seed/generation returns 404. Existing startup pin, retained-generation
scope, loopback/browser restrictions, and worker admission still apply.

Return the existing schema-1 HTTP envelope with JSON `data` containing ordered
`nodes` (`depth`, typed `node`), `edges` (`depth`, typed `edge`), examined incidence
count, accounted item bytes, and explicit `truncated`. Stored edge direction is
retained although traversal is weakly connected. Reaching requested hop depth is
intentional, not truncation. Results are not pageable; narrower/wider requests
are separate bounded traversals. Truncation does not imply complete impact.

The HTTP output sizer reserves a maximum-width empty envelope/summary, counts
typed item JSON with conservative separators, and bounds writes without building
export records. Verify final response length against the requested body cap;
return 413 if metadata/seed cannot fit and fail closed on accounting violations.
Caps cover successful JSON body bytes, not error responses, HTTP headers, or
total database memory/latency.
The store still owns indexed history reads and BFS work limits.

## Verification

Existing query/export tests pass unchanged. The existing HTTP pagination TCP
fixture now also checks current/retained multi-hop results against independent
export-path queries at one, two, and three hops, preserving typed facts, depths,
direction, and scan/truncation summaries. Node/edge/scan caps report truncation;
successful byte-limited responses fit their requested cap. Too-small metadata/
seed budgets return 413. Invalid caps/options/generation IDs return 400, removed
or missing seeds/generations return 404, and a writer advancing startup causes
409. The JSON counter handles escaping and exact-size boundaries without building
output. Focused query/HTTP tests and strict Clippy pass. Full `cargo make ci`
passes on this implementation (143.31 seconds), including all-feature workspace
tests, migration/extension conformance, architecture checks, strict Clippy,
dependency audits, and documentation builds. Existing bincode/paste unmaintained
exceptions remain unchanged. This is local Linux evidence, not complete
HTTP/daemon or hosted-platform acceptance.
