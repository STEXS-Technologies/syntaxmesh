# ADR 0382: Incremental advancement of retained contract history

Status: accepted

Date: 2026-10-22

## Context

The first durable contract-history implementation accepts a complete contract
batch for one pinned graph generation. A caller must currently resend the same
declarations for every generation, and a request spanning several accepted
generations must manually replay that work. Each retained result already stores
its authored contract, while canonical graph history exposes bounded ordered
generation-change pages.

## Decision

Add an explicit Engine catch-up operation that reads the latest complete,
integrity-checked contract evaluation set from the downstream inference
materializer, then evaluates that exact retained contract set over every
immediate accepted successor through a caller-pinned generation. It does not
accept replacement declarations; contract authoring/version changes remain an
explicit separate operation. It rejects a target before the retained contract
baseline and preflights the generation distance against a caller-supplied bound
before any new effect.

Expose catch-up in the feature-enabled CLI for File and Turso canonical stores.
The CLI accepts the target generation and independent generation, history-scan,
input, edge-visit, and per-generation output budgets; it acquires writer leases
for both stores and requires explicit prior inference migration.

Each successor result is independently journaled through Penelope before its
SQLite publication. Therefore a crash can leave a correct retained prefix, and
retry resumes from the latest complete generation while Penelope recovery
finishes any prepared publication. Every transition is checked for contiguous
sequence, expected parent, repository, and worktree. Reads of the latest
complete set verify its row count, payload hashes, aggregate digest, contract
identities, and deterministic operation identity.

The operation is explicit catch-up, not a hidden side effect of canonical graph
publication. This preserves separate derived-state storage and lets hosts
choose when and how much semantic work to schedule.

## Consequences

- Hosts can advance already-authored contracts over missed generations without
  resending declarations or replaying from the original baseline.
- Work is bounded before effects; each completed generation remains queryable
  even if a later one fails.
- Contract authoring, automated scheduling at publication, definition lifecycle,
  richer predicates, and drift summaries remain open.

## Verification

Verified complete-set integrity and restart reads; Engine and File/Turso CLI
catch-up over multiple transitions; generation-budget rejection before new
effects; idempotent no-op at the retained head; and recovery after an injected
failure between per-generation Penelope-journaled publications. The focused
CLI suite and full non-DuckDB workspace CI passed on 2026-10-22.

## References

- ADRs 0376–0381.
- SyntaxMesh source of truth v5, sections 82, 94–96, and 230–239.
