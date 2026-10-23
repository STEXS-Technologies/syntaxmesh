# ADR 0395: Verified inference explanations

Status: accepted

Date: 2026-10-23

## Context

The inference Query API can resolve exact retained premises and independently
verify a rule witness, but callers must coordinate those calls and source
provenance themselves. This makes it easy to display a derived claim without
its evidence or to confuse a serialized claim with a verified proof. The v2
reasoning plane requires explanations to identify the conclusion, rule, exact
premise versions, source evidence, and generation.

## Decision

Add a storage-independent, versioned `InferenceExplanation` DTO and a bounded
Query operation that returns it only after validating the claim's identity and
re-evaluating its witness against the selected canonical generation. Bundle
premises in the claim's deterministic order, each with its exact fact-version
reference, retained edge, and producer provenance (including source spans when
available). Bound the operation by premise count, proof depth, and serialized
output bytes.

The DTO describes one directly verified deterministic-rule witness. It is not a
recursive derivation DAG, does not establish truth-maintenance status or
contradiction completeness, and does not use an LLM. Existing `Claim` and
inference-page schemas remain unchanged; the explanation has its own schema
version. The API model depends only on core types, and Query depends on no
database, workflow, transport, or host implementation.

## Consequences

- v2 clients can consume proof evidence as one coherent bounded result.
- Exact provenance stays coupled to its premise in output, preventing callers
  from assuming unverified claims have authoritative explanations.
- The result is unavailable if retained premise or provenance evidence is
  missing, corrupt, outside the selected generation, or exceeds bounds.
- Recursive explanations, historical claim state, host/CLI/MCP delivery, and
  model-generated summaries remain future work.

## Verification

API-model round-trip tests cover schema version, identity, ordered premise
evidence, and provenance. Query tests cover valid explanations, altered claims,
inactive/foreign premises, missing evidence, and premise/depth/output bounds.
Architecture checks preserve the pure DTO and runtime-neutral Query boundaries.

## References

- SyntaxMesh source of truth v5, sections 77, 80, 89, and 316.
- ADRs 0361–0367 and 0376–0379.
