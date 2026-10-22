# ADR-0330: Restricted inference above canonical facts

Status: initial pure evaluator implemented; host, source integration and release gates open.

## Initial evaluator contract

The first pure evaluator accepts scoped, generation-labeled edge premises and
node identities, plus one explicit rule: a nonempty path of one relation, joined
to a second relation and a terminal concept-declaration relation. Rule identity,
version, ontology versions and configuration fingerprint are explicit inputs.
Each separate supporting witness produces a derived claim with exact edge
version premises, scope and generation. Output relations must be extension-owned.
The initial schema-1 identity hashes domain-separated deterministic JSON of the complete rule,
scope, generation, output endpoints and ordered witness; JSON here is an in-memory
identity encoding, not storage or service communication.

ADR-0335 advances this optional claim identity to schema 2, incorporating an
explicit deterministic derivation class. Canonical source IDs are unchanged.

Input facts, edge visits, path depth and outputs have mandatory positive bounds.
Repeated nodes in a witness are detected and skipped; acyclic depth exhaustion
fails rather than returning an apparently complete prefix. Generation labels and
duplicate/dangling facts reject before execution. A pure evaluator cannot verify
ancestry of premise valid-from values: the future host must obtain them from
authoritative retained versions, not manufacture them from current edge IDs.
No durable materialization, ontology discovery or general rule language is implied.

The initial path/join tests pass for transitive witnesses, input-order invariance,
premise removal, identity changes and fail-closed work/depth/output limits and
mixed generations. Architecture dependency checks pass with inference explicitly
classified as pure/runtime-neutral. Supported-language extraction integration,
cycle/invalid-input fixtures and full contributor CI remain required; this is
not yet a delivered end-to-end reasoning workflow.

Follow-up verification: five unit tests and one real Rust extractor/resolver
integration pass. The extracted checkout/payment/writer call path combines with
explicit domain declarations to yield an exact four-premise witness. Removing
the declaration removes claims without removing source call facts. Cycles skip
repeating nodes while preserving finite witnesses; duplicate and dangling facts,
generation mismatches, reserved outputs and exhausted bounds reject. Strict
all-target inference Clippy and architecture checks pass. This fixture labels
fresh extraction versions itself, not authoritative durable history; retained
version host integration and durable premise invalidation remain open.

Contributor CI for the pure evaluator and real Rust integration passes in
146.19 seconds. The later optional Query adapter is outside that completed test
run and requires its own lifecycle verification (ADR-0331).

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
