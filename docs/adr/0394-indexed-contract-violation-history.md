# ADR 0394: Indexed contract violation history

Status: accepted

Date: 2026-10-23

## Context

Retained contract outcome timelines expose every observed episode, but callers
cannot directly request violation history. A violation must be evidence-backed:
`Contradicted` records an observed forbidden relation; `InsufficientEvidence`
does not establish either compliance or violation. Scanning all outcome episodes
in application code would also make sparse violation history proportional to
unrelated outcomes.

## Decision

Add a read-only File/Turso CLI and Engine query for the bounded chronological
episodes whose retained outcome is `Contradicted`. Preserve the existing
all-outcome timeline unchanged. The downstream inference SQLite adapter filters
episodes in SQL and adds a partial ordered index restricted to contradicted
rows. Pagination cursors remain scoped to repository, worktree, contract, and
version, and are validated against the filtered result set.

The result describes observed evaluation generations only. It does not treat
`InsufficientEvidence` as a violation, claim unevaluated generations are clear,
or infer why a violation occurred. This is a read path over existing Penelope-
retained history; it adds no canonical graph replay or mutation workflow.

## Consequences

- v5 temporal contract analysis gains a direct `ViolationHistory(contract)`
  operation with bounded keyset pagination.
- Sparse violation reads seek the partial index instead of scanning all
  episodes for a contract.
- Existing outcome-history and as-of contracts remain unchanged; no result
  schema or temporal interpretation changes.
- Resolution/deprecation lifecycle and consequence attribution remain future
  work.

## Verification

Migration tests verify upgrade atomicity and fresh-schema index registration.
SQLite query-plan coverage asserts the filtered episode lookup uses the partial
index. Materializer, Engine, and File/Turso CLI tests cover outcome filtering,
cursor continuation and scope rejection.

## References

- SyntaxMesh source of truth v5, sections 269 and 316 Stage C.
- ADRs 0380–0386 and 0393.
