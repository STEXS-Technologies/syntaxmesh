# ADR-0390: CLI access to retained contract evaluation history

Status: accepted

Date: 2026-10-23

## Context

The inference materializer and Engine already expose bounded keyset pages of
retained contract evaluations. CLI users can read one exact/as-of result or
outcome episodes, but cannot inspect every retained evaluation for one
contract/version without using a library or direct database access.

## Decision

Add read-only `inference-contract-history[-turso]` commands. Each command is
scoped by the canonical store's repository and worktree, contract ID/version,
and a page limit capped by the existing materializer. An optional cursor JSON
file continues the indexed keyset page and remains bound to that scope.
Output is versioned JSON containing only retained evaluation generations and
the next cursor. The command does not replay canonical history, mutate either
store, or imply that an older evaluation describes the latest graph state.

## Consequences

- Operators can inspect the full indexed history using the same runtime-neutral
  Engine API as embedded callers.
- Pagination remains bounded and cursor misuse fails closed in the existing
  materializer.
- No schema migration or public DTO change is required.

## Verification

File and Turso CLI tests page across multiple retained generations, verify
ordered complete results and cursor continuation, and reject a cursor bound to
another contract. Query output remains unchanged after reads.
