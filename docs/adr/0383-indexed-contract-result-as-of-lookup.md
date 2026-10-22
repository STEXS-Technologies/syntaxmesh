# ADR 0383: Indexed contract-result lookup at a pinned generation

Status: accepted

Date: 2026-10-22

## Context

Contract evaluations are retained by contract identity and accepted graph
generation. Exact-generation lookup and chronological pages are available, and
the Engine can explicitly catch a retained contract set up across immediate
successors. Consumers still need a direct temporal point lookup for the latest
evaluation at or before a selected generation, without paging or replaying all
intervening history.

## Decision

Add an indexed read that selects the latest retained evaluation for one
`(repository, worktree, contract_id, contract_version)` whose generation
sequence is no greater than the selected canonical generation sequence. Use
the existing contract-history index in descending order with a one-row limit;
do not add a migration or duplicate materialized state.

The result includes both the requested canonical generation and the actual
evaluation generation. A missing result is unavailable; a result from an older
generation is stale at the requested generation. This API reports the latest
retained conclusion only. It must not imply that an older result is the
contract's compliance state at a later generation, and it never evaluates,
replays, or advances history.

The runtime-neutral Engine validates that the requested generation exists in
its repository/worktree and supplies its accepted sequence. Expose bounded
File/Turso CLI reads using the same Engine API. The downstream SQLite query
validates payload checksum and identity before returning the record.

## Consequences

- A temporal point lookup uses one indexed range seek instead of paging through
  earlier results; it does not claim constant asymptotic cost in index size.
- Consumers can distinguish an exact-generation evaluation from a stale prior
  evaluation without confusing absence of a newer evaluation with compliance.
- No schema change, canonical-store write, automatic scheduling, or new
  contract lifecycle semantics are introduced.

## Verification

Verify exact, prior, missing, and future-excluded results across retained
generations; reject unknown/foreign target generations through Engine; verify
the SQLite query uses the existing history index and detects payload tampering;
compare File/Turso CLI output and confirm reads do not mutate either store.
Focused package tests and strict Clippy passed; full `cargo make ci` passed in
167.72 seconds on 2026-10-22, with the opt-in DuckDB adapter excluded.

## References

- ADRs 0376–0382.
- SyntaxMesh source of truth v5, sections 269 and 316.
