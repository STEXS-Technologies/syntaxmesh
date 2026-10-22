# ADR 0377: Generation-pinned contract query

Status: accepted

Date: 2026-10-22

## Context

ADR-0376 introduced pure contract evaluation over a caller-supplied selection.
That is useful to embedding callers but leaves each adapter to independently
load retained graph facts, risking incomplete or mixed-generation evidence.
The Query layer already owns bounded, integrity-checked reads of active fact
versions for deterministic inference.

## Decision

Expose contract evaluation through `Query` when its existing `inference` feature
is enabled. The operation uses the Query's pinned generation, loads the
generation's active canonical fact versions through the existing paged history
reader, enforces a separate history-scan budget, then delegates to the pure
contract evaluator and its independent count/byte/edge-visit/result bounds.
Rule inference and contract evaluation share the same validated selection
loader. Results cite retained fact-version openings and the exact selected
generation. The API remains read-only and does not assign severity, persist
results, or claim coverage beyond the loaded graph.

## Consequences

- CLI, MCP and embedded clients can build on one canonical generation-pinned
  semantic query instead of duplicating history loading.
- Contract evaluation now consumes active retained store facts rather than
  depending on callers to manufacture a `Selection` correctly.
- Host exposure, contract authoring/persistence, policy, and temporal result
  history remain separate follow-up work.

## Verification

Exercise current and retained generations, exact evidence versions,
insufficient-evidence outcomes, malformed history rejection, and independent
history/evaluator bounds through Query tests. Run Query strict Clippy and the
full workspace CI gate.

## References

- ADR-0376, evidence-bounded semantic contract evaluation.
- SyntaxMesh source of truth v5, sections 82, 86, 89, and 269.
- `crates/syntaxmesh-query/src/service/inference.rs`.
