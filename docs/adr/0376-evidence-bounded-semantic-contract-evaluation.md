# ADR 0376: Evidence-bounded semantic contract evaluation

Status: accepted

Date: 2026-10-22

## Context

The v2 product requires contracts to be compared with source, schema, config,
declaration, documentation, and runtime evidence. Current canonical selections
contain retained facts, but absence from a static selection does not establish
that a behavior is impossible or that a contract is violated. Contract results
must retain this uncertainty rather than collapse evidence and assumptions.

## Decision

Add a pure, bounded evaluator in `syntaxmesh-inference` for explicit relation
contracts over a caller-selected graph generation. A contract requires or
forbids one exact relation between two node identities and records its origin.
An exact retained edge supports a required relation and contradicts a forbidden
relation. No matching edge produces `InsufficientEvidence`, never success or
violation. Each result cites at most one exact retained fact version.

Evaluation is bounded by contract count, serialized input bytes, and total edge
visits. Results are generation-scoped and deterministically identified; the
operation does not mutate canonical facts, persist results, assign severity, or
make model-suggested/documented/runtime-inferred origins hard-enforceable. Hard
enforcement policy, richer predicates, durable contract history, and temporal
drift analysis remain separate work.

## Consequences

- Deterministic relation contracts can be evaluated now without pretending the
  restricted inference rules are a general policy engine.
- Callers can distinguish positive evidence, counter-evidence, and missing
  evidence; downstream tools must preserve these states.
- This is a v2 contract-evaluation primitive and a prerequisite for drift and
  v5 contract history, not completion of those capabilities.

## Verification

Cover required/forbidden outcomes, exact evidence version and generation,
insufficient evidence, invalid/duplicate contracts, deterministic identity,
and independent count/byte/edge-visit budget exhaustion. Run the inference
crate tests and strict Clippy.

## References

- SyntaxMesh source of truth v5, sections 82, 86, 89, and 269.
- `crates/syntaxmesh-inference/src/contracts.rs`.
