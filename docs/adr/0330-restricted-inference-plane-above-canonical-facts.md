# ADR-0330: Restricted inference above canonical facts

Status: proposed; no public inference contract or implementation yet.

## Requirement and existing boundary

Source-of-truth sections 77 and 79 require an optional deterministic reasoning
plane, independent of structural indexing. Each result binds rule identity and
version, exact premises, ontology versions, source generation and configuration
fingerprint. Execution must have explicit resource/depth bounds and no arbitrary
filesystem, network, process or user-code access. Inference must not overwrite
extracted facts or flatten derivation classes into probabilistic confidence.

The existing `ConsequenceDelta::validate` accepts only explicit derivations.
`DerivedFrom` representation and retained lineage queries do not constitute a
rule evaluator. Do not relax this validator to make inference appear supported.

## Reuse decision

Reuse Slotstrike's separation of pure match specifications from orchestration
(`src/domain/specifications/rule_matching.rs`) and explicit match-source labeling
(`src/domain/services/rule_matcher.rs`). Reuse SyntaxMesh's typed IDs, pinned
generation checks and provenance conventions. Do not copy Slotstrike's business
rules, first-match-wins behavior or inline test layout.


## First vertical slice and acceptance evidence

Introduce the docs' suggested `syntaxmesh-inference` as a pure optional crate,
not logic inside Engine, Query, host or canonical DTO modules. Start with an
inspectable bounded relation-path/join evaluator, not a general language runtime.
Inputs are explicit pinned facts and rules; outputs are separate derived claims
and exact premise records. Missing premises mean no claim, not an invented edge.

Before public types are finalized, verify namespace/version identity, supported
relation binding and schema/serialization evolution against the original docs.
The first runnable slice must exercise a supported-language call path joined
with an explicit domain declaration, not merely construct a derivation DTO.

Required tests: deterministic input-order invariance and IDs; rule/configuration/
ontology-version changes alter derivation identity; cycle/depth/work exhaustion
fail explicitly without partial success; premise removal removes the derived
claim while leaving original facts intact; generation mixing rejects. Keep tests
in sibling modules. Both extraction-only and inference-enabled workflows must
remain usable without AI or another runtime.

Durable materialization and truth maintenance are subsequent Engine workflows
using Penelope and existing generation verification, not side writes by the pure
evaluator. On-demand evaluation comes first; do not precompute unbounded closures.
This slice advances v2's reasoning plane and creates a prerequisite for v3
reconciliation and v5 premise/consequence invalidation. It does not complete any
of those versions or fix v0 lexical retrieval.
