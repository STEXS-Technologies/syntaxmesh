# ADR-0178: HTTP historical neighbor pages

- Status: accepted
- Date: 2026-09-30

## Decision

Add GET `/api/v1/nodes/{id}/neighbors` to the existing read-only HTTP host.
Reuse `Query::historical_neighbors`, the generation-scoped incidence index,
and `HistoricalNeighborCursor`; add no host graph algorithm or canonical format.
Both current and retained generations use the same indexed query operation.
Optional query fields are `generation` (64 hex characters), `direction`
(`outgoing` default or `incoming`), `limit` (1–100, default 20), and `cursor`.
The endpoint must exist at the selected generation; otherwise return 404.

Reuse Shardline's bounded, versioned, URL-safe unpadded Base64 JSON cursor
pattern. Encode `{version: 1, cursor: HistoricalNeighborCursor}` as an opaque
token limited to 2,048 bytes. Reject oversized input before decoding, malformed
encoding/JSON, unsupported version, and cursors bound to another generation,
endpoint, or direction with 400. Cursor tokens are continuation coordinates, not
signed claims or authorization. Existing Host/origin, 8,192-byte URI, admission,
scope, and pre/post startup-generation checks remain authoritative.

The existing HTTP envelope's `data` contains hex `endpoint`, typed `direction`,
`items` (typed `edge` and `neighbor` pairs), `has_more`, and nullable opaque
`next_cursor`. Pass the returned token unchanged as `cursor` to continue with the
same generation/endpoint/direction; page size may change. No NDJSON intermediary,
history replay, `GraphAt` materialization, new migration, or publication workflow.
This closes one paged navigation surface, not bounded multi-hop HTTP/context or
daemon refresh/authentication acceptance. Cost is indexed seeks plus returned
facts, not strict O(1) in graph size or output.

## Verification

Real TCP fixtures pass page-by-page equivalence with independent Engine queries
at both current and retained generations, for outgoing/incoming direction and
isolated endpoints. Returned cursor coordinates match the existing typed query
continuations. Changing the page size while resuming preserves results. Removed
endpoints remain readable at their retained generation and return 404 currently.
Malformed/oversized/versioned cursors, mismatched generation/endpoint/direction,
unknown options, and invalid limits/directions/generation IDs return 400. Publication after
startup causes 409 rather than mixed-generation output. The cursor codec test,
strict host Clippy, and architecture checks pass. Full `cargo make ci` passes on
this implementation (139.65 seconds), including all-feature workspace tests,
migration/extension conformance, strict Clippy, dependency audits, and docs.
Existing bincode/paste unmaintained-dependency exceptions are unchanged. This is
local Linux evidence, not hosted-platform or complete daemon acceptance.
