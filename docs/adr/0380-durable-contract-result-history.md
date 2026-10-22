# ADR 0380: Durable generation-scoped contract result history

Status: accepted

Date: 2026-10-22

## Context

The v5 roadmap's Stage C requires historical contract results. The current
evaluator returns a generation-pinned result but retains neither the authored
contract nor its result after the request. Replaying every retained generation
to answer a historical contract query defeats the indexed temporal-history
architecture. Canonical graph facts must remain independent from downstream
contract evaluation.

## Decision

Add an explicit downstream contract-result materializer alongside the existing
inference materializer. It records the exact contract declaration, its
generation-scoped `ContractEvaluation`, repository/worktree scope, accepted
generation sequence, and deterministic materialization identity. A complete
batch is published atomically and idempotently to SQLite through an explicit
registered migration. Reads by `(repository, worktree, contract_id, version,
generation)` and ordered history pages use dedicated indexes; they do not replay
canonical graph history. Re-evaluating a different result for an already
retained contract/version/generation is an integrity conflict, not an overwrite.

The pure `syntaxmesh-inference` and Query contracts remain storage-free. Full
Engine execution is journaled through Penelope before the downstream SQLite
effect; prepared retries must converge on the identical receipt. StateChronicle
remains opt-in verification of the canonical generation manifest chain: it
does not alter evaluation outcomes or substitute for the durable result store.
The feature-gated Engine exposes the journaled write plus exact-generation and
bounded history reads; inference recovery reports recovered contract-history
work separately from claim baselines and stream effects.
Contract-result history is derived and rebuildable from canonical generations
and authored contract inputs; it is not canonical graph truth or a compliance
enforcement decision.

## Consequences

- Historical results become direct indexed reads after one evaluation has
  been materialized; unmaterialized generations remain explicitly absent and
  are never silently filled by request-time replay.
- Contract inputs, results, and evidence references are retained together,
  allowing later policy/schema changes to create new contract versions without
  rewriting earlier conclusions.
- This begins v5 Stage C only. Change-to-change causality, full contract
  lifecycle semantics, policy enforcement, and long-range drift analysis remain
  separate work.

## Verification

Test explicit migration and tamper detection, atomic complete publication,
idempotent retry, changed-result conflict, indexed exact-generation and ordered
history reads, backend restart behavior, Penelope prepared recovery, and
StateChronicle on/off invariance of canonical and contract-result semantics.

## References

- SyntaxMesh source of truth v5, sections 301, 311, 316, 317, and 320.
- ADRs 0026, 0027, 0346, and 0376–0379.
