# ADR 0237: Owned retained fact-history HTTP pages

Status: accepted

## Decision

Add GET `/api/v1/fact-history` through existing shared Engine admission.
Optional `generation` selects retained history; `limit` defaults to 100 and is
restricted to 1–100. Continuations supply all of `generation`, `after_kind`
(file/provenance/node/edge), `after_id`, and `after_valid_from`.
Reject incomplete, malformed, or unknown query fields before querying.
Return schema version 1, generation, repository/worktree, globally ordered items
with typed fact identity, sequence bounds and existing temporal FactVersion
records, and an optional typed `next_cursor`. Hex cursor IDs are explicit.
Serialize through the existing 4 MiB bounded JSON writer (oversize: 413).

## Limits

Reuse Query/store paging and durable history indexes; no replay or database
reopening in the host. This is a global history feed, not identity-filtered
history attachment. A single oversized fact can fail a page. Reference stores
retain their existing scanning behavior; complete exports are not O(1).

## Verification

Full `cargo make ci` passes in 213.04 seconds, including strict workspace Clippy,
architecture/migration checks, 33 query tests, ten HTTP tests, and the 16 backend
conformance scenarios. The real TCP history fixture walks one-item current and
retained pages and compares payloads, sequence bounds, scope, and continuations
against the Engine. It rejects malformed/incomplete/unpinned query cursors,
unknown fields, invalid limits, and unavailable generations. The existing bounded
writer tests verify the shared serializer's exact-size/oversize behavior; the
history fixture does not inject a history-specific oversized fact.
