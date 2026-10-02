# ADR-0227: Read-only workflow rejection HTTP

- Status: accepted
- Date: 2026-09-30

## Decision

Expose GET `/api/v1/workflow-rejections` through the existing shared Engine,
admission, Host/Origin checks, and owner-instance boundary. Read the existing
Penelope rejection journal without recovery, publication, or a new connection.
Parameters are `limit` (default 20, range 1–100) and exclusive `after_run`.
Unknown/malformed parameters return 400.

Return the version-1 envelope with the current generation observed under the
same Engine lock, plus `items` and `next_cursor`. Each item contains hexadecimal
`run_id` and a tagged reason: `unknown_legacy`, or `stale_base` with repository,
worktree, expected, and actual hexadecimal IDs (nullable generations).
Workflow journal pagination is not a historical graph snapshot: generation is
an observation label, not a pin for subsequent journal reads. Successful JSON
responses use the existing 4 MiB bounded serializer; oversized pages return 413.

Reuse the retained Rust HTTP-client pattern already adapted from Shardline for
future CLI attachment. CLI rejection output and limit semantics must stay intact;
this route alone does not close that attachment gate. No DTO dependencies are
added to pure core or runtime protocol crates. No NDJSON service handoff.

## Verification gates

The shared TCP fixture passes with two actual Engine publication attempts
rejected by Penelope for stale bases. One-item HTTP pages match the existing
Engine rejection reader's typed reasons and deterministic order. Inspection
leaves the full rejection page and workflow counts unchanged. Empty pages and
invalid limits/cursors/unknown parameters are also checked over TCP.
These checks do not establish journal snapshot isolation or total scan-cost
bounds; the existing reader may scan non-rejected workflow records.

Check empty and typed rejection pages against the existing Engine reader,
exclusive cursor semantics, invalid parameters, and unchanged durable state.
CLI attachment and byte-equivalent export remain follow-up work.
