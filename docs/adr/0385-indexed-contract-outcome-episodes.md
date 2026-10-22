# ADR 0385: Indexed contract outcome episodes

Status: accepted

Date: 2026-10-22

## Context

Retained contract results already support indexed exact-generation and
as-of-generation reads, plus bounded chronological pages. They do not provide
a compact answer to when the currently observed outcome first appeared. A
client would have to page through prior results, even though the result history
is already materialized and append-only.

Outcome history is evidence at evaluation generations, not proof that the same
state held continuously between evaluations. An older `Supported` result must
remain stale at a later unevaluated generation. In particular, SyntaxMesh must
not report elapsed wall-clock nonconformance from generation distance or imply
that a retained outcome is current without an exact evaluation.

## Decision

Add a downstream, indexed projection that groups consecutive retained
evaluations for one `(repository, worktree, contract ID, contract version)` by
their exact `ContractOutcome`. Each episode records its first and last observed
generation and sequence. A transition to a different outcome starts a new
episode. Publication of a complete replacement set ends episodes for omitted
contracts; reintroduction starts a new episode. Episode changes are committed
atomically with the retained evaluation set.

Migration backfills the projection from retained sets/evaluations in accepted
sequence order and fails atomically on invalid retained payloads. A bounded
indexed as-of query selects the episode corresponding to the latest retained
evaluation no later than the requested generation, without scanning canonical
history or paging the full contract history. The Engine exposes a
runtime-neutral summary; File/Turso CLI output includes target and evaluation
coordinates and explicitly labels the result `current`, `stale`, or
`unavailable`.

The episode reports first/last observation only. It does not claim continuous
conformance, calendar duration, severity, acknowledgement, or policy. Missing
evaluation generations remain unknown, not compliant. Contract-version
replacement remains separate from outcome changes within one version.

## Consequences

- Repeated same-outcome generations have a compact temporal summary while exact
  per-generation results remain available and unchanged.
- A one-row indexed lookup supplies the episode for an as-of result; no
  canonical graph replay or full result-history scan is required.
- SQLite migration work and transactional episode maintenance are added to the
  downstream inference adapter; no core/public DTO or canonical graph schema
  gains a database dependency.
- Lifecycle statuses such as acknowledged, accepted, resolved, and deprecated
  remain future work and require separate evidence and policy semantics.

## Verification

Migration rollback and checksum registry pass. SQLite fixtures cover first,
repeated, changed, omitted, and reintroduced outcomes; complete-set backfill;
exact retry and atomic failure; and the indexed as-of query plan. File/Turso CLI
tests verify stale labeling and observed-generation bounds. The inference SQLite
(29 tests), Engine (25 tests), and CLI inference suites (5 tests) pass, along
with feature-enabled strict Clippy. Full `cargo make ci` passed in 154.00
seconds on 2026-10-22, covering formatting, architecture and migration gates,
workspace checks/Clippy, dependency/security checks, tests, doc tests, and API
docs. DuckDB remains excluded from routine CI. Historical result payloads are
unchanged; no result is inferred at an unevaluated target generation.

## References

- ADRs 0380–0384.
- SyntaxMesh source of truth v5, sections 270, 294, and 316.
