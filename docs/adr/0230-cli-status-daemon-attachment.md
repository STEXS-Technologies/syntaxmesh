# ADR-0230: CLI status daemon attachment

- Status: accepted
- Date: 2026-09-30

## Decision

GET `/api/v1/status` returns the existing Engine logical-integrity status and
Penelope workflow counts in a version-1 generation envelope. Optional boolean
`include_files` includes the complete indexed FileVersion inventory from that
same generation. Unknown/malformed parameters return 400. Engine status,
workflow counts, and inventory are read under one existing admitted Engine lock.
Reuse the boundary and bounded JSON serializer, with a 4 MiB successful-response
limit and explicit 413 rather than partial inventories.

Attach `status-turso` through existing guarded owner discovery/client. Keep the
embedded formatter and its integrity/index-lag exit behavior. For a supplied
source root, scan locally, validate repository/worktree against the returned
manifest, and use the shared pure freshness comparison. Validate envelope/manifest
generation, inventory presence/count, and unique file IDs before output.
Absent ownership retains a real lease; active errors never reopen Turso.

Logical integrity still scans the full graph. This is not a cheap health check,
constant-time status, or atomic source-scan snapshot. Large inventories exceeding
the byte limit explicitly fail; paged inventory transport remains a scalability
gate. The existing client timeout can fail slow checks without cancelling the
already admitted blocking worker. No database/workflow dependencies enter core
or public DTO/runtime protocol crates, and no NDJSON service handoff is added.

## Verification gates

Compare embedded/attached output and exit results for unchecked/current/stale
freshness, foreign roots, and post-publication refresh. Reject mismatched
generations and incomplete/duplicate inventories before output. Full CI remains
required before claiming this attachment complete.

The owned live Engine fixture passes for byte-identical unchecked/current
embedded and attached reports. After editing Rust and TypeScript sources,
attached status reports stale freshness and fails; after reconciliation it
reports current freshness and succeeds. Strict affected all-feature Clippy
passes. Malformed response fixtures reject mismatched generations and missing,
incomplete, duplicate, or unsolicited inventories with no output or fallback.
The live fixture confirms byte-identical foreign-root errors and post-shutdown
leased status fallback. HTTP rejects invalid include-files values and unknown
parameters. Full `cargo make ci` passed in 194.58 seconds, including strict
workspace all-feature Clippy, architecture/migration/dependency gates, extension
isolation, 13 CLI attachment tests, 10 HTTP tests, 16 backend-conformance tests,
and API documentation. Large-inventory paging and cheap/cached integrity status
remain open; these results establish correctness for the tested bounded fixtures.
