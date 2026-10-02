# ADR-0226: Bounded diagnostic query and attachment

- Status: accepted
- Date: 2026-09-30

## Decision

Expose a generation-scoped diagnostic page query using historical node pages.
The scan limit bounds examined nodes, not matching diagnostics. Empty pages can
have a continuation; that continuation is the last scanned node, not the last
matching diagnostic. Preserve all diagnostics across such pages.

Add GET `/api/v1/resolution-diagnostics` with optional `generation`, `after_node`,
and `scan_limit` (default/max 100). Continuations require an explicit generation.
Return the existing version-1 JSON envelope with repository, diagnostic items,
scanned-node count, `has_more`, and `next_after`. Read manifest and page under
one admitted Engine query. Reuse Host/Origin/owner-instance checks and admission.
Successful responses are capped at 4 MiB; oversized pages fail with 413.

Attach `resolution-diagnostics-turso` using one retained guarded client, pinning
generation/repository after the first page. Validate counts, diagnostic types,
ordering and forward continuations, including empty nonterminal pages. Collect
everything before exporting through the same CLI header/item/footer formatter.
Active errors never fall back; absent ownership retains a real writer lease.
NDJSON remains explicit CLI export only, not internal communication.

This initial path scans retained nodes in bounded requests. It does not provide
a dedicated kind index, constant-time diagnostic lookup, or bounded total CLI
memory/time. Dedicated diagnostic indexing remains an optimization gate, and
oversized single-page diagnostics are reported rather than silently truncated.

## Verification

The shared TCP neighbor fixture also verifies diagnostic scan pages against
independent Engine queries for old and current generations after publication.
One-node pages exercise empty-match continuation through retained node versions.
It rejects malformed IDs, invalid/oversized scan limits, unknown parameters,
continuations without generation (400), and unknown generations (404).
The route-level fixture publishes small and oversized versions of the same
diagnostic into Turso. The oversized current request returns 413, while explicit
old-generation selection returns exactly the retained nonempty diagnostic.
All 10 HTTP tests and affected HTTP all-feature Clippy pass after these follow-ups.
This is protocol/storage evidence using operator-asserted fixture facts, not a
representative source-resolution quality evaluation.

Query unit tests pass for empty nonterminal progress, last-scanned continuation,
terminal empty pages, and impossible backend continuation. The bounded JSON
serializer test accepts the exact byte limit and returns 413 one byte below it.
CLI attachment tests reject invalid scan counts, changed generation/repository,
nonadvancing cursors, and missing/wrong/duplicate owner identity on later pages,
with no output and unchanged invalid database bytes proving no embedded fallback.
The live Turso fixture verifies a nonempty unresolved TypeScript diagnostic export
is byte-identical to embedded output across the wide historical node scan and
refreshes to no diagnostic matches after source reconciliation.
Full `cargo make ci` passed for the nonempty fixture in 193.37 seconds, including
strict workspace all-feature Clippy, architecture/migration gates, extension
isolation, 11 daemon-attachment tests, 16 backend-conformance tests, and API docs.

Require byte-equivalent CLI exports for empty and nonempty diagnostic sets,
multiple node-scan pages, live refresh, retained HTTP generation selection, and
leased fallback. Validate empty nonterminal progress and reject foreign scope,
generation, node kinds, counts, order, and nonadvancing continuations before output.
