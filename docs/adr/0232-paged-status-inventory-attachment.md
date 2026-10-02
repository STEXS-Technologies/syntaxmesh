# ADR-0232: Paged status inventory attachment

- Status: accepted
- Date: 2026-09-30

## Decision

Expose GET `/api/v1/files` with optional generation, exclusive `after_file`, and
limit (default/max 100). Continuations require explicit generation. Return the
version-1 envelope with repository/worktree, FileVersion items, `has_more`, and
`next_after`. Reuse historical file pages, existing query admission/owner guards,
and the 4 MiB bounded serializer; oversized pages return 413.

CLI status always requests the existing compact report without inline files.
When freshness is requested, fetch file pages using the same retained guarded
client and the report's explicit generation. Validate scope, count, strict
exclusive ordering, and continuation matching the last item of a full page.
Only print after collecting exactly the report's file count. Zero-file reports
still fetch one page to verify the inventory. Publication between pages cannot
mix graph generations. Keep the old inline-files HTTP option for compatibility.

This removes the single-response inventory ceiling for CLI status, not total
inventory memory/time or the limit on a single large file fact. Logical integrity
remains a full graph scan. No replay, schema migration, or NDJSON service handoff.

## Verification gates

Verify multi-page inventory equivalence, refresh, retained reads, malformed scope,
generation/count/order/continuations, later-page identity guards, and full CI.

The live-owner fixture passes with more than 100 indexed files, exercising
multiple inventory requests, byte-identical status output, stale/current refresh,
foreign-source rejection, and leased fallback. Malformed status/file response
fixtures cover mismatched generations, duplicate/missing inventory entries,
foreign repository, and short nonterminal pages before output. Complete
attachment tests pass. Later inventory requests additionally reject missing,
wrong, and duplicate owner response identities before output with unchanged
invalid database bytes. TCP inventory tests compare old/current pages against
the Engine after a file update and deletion; the retained inventory exercises
multi-page continuation. Invalid limits/IDs/unknown parameters and continuation
without generation return 400; unknown generations return 404. Targeted tests
and strict affected all-feature Clippy pass. The existing publication proxy is
shared with a status fixture: after a real nonterminal first inventory page it
updates Rust source, deletes an inventory file, and publishes a new generation
before returning the page. CLI output remains byte-identical to the original
embedded generation while the current graph demonstrably changes. Both neighbor
and inventory publication fixtures pass. Full `cargo make ci` passed in 236.37
seconds, including strict all-feature workspace Clippy, architecture/migration
and dependency gates, extension isolation, 14 attachment tests, 10 HTTP tests,
16 backend-conformance tests, and API documentation.
