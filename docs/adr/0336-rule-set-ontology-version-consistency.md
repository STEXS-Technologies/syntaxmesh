# ADR-0336: Rule-set ontology version consistency

Status: implemented for explicit rule-set bindings.

The deterministic rule-set API represents one selected ontology configuration.
Reject empty ontology namespace/version identifiers and conflicting versions
for the same namespace across that set before executing any rule. Distinct
namespaces and consistent shared versions remain valid. An empty map is valid
for rules not using an ontology; this does not verify an ontology's existence
or semantic compatibility. Comparing different ontology configurations requires
separate explicit evaluations, not silently merging their outputs.

Single-rule evaluation must use the same validation path. Preserve stable claim
identity for valid existing configurations; add no runtime or registry dependency.
Version-binding validation work is bounded by the existing edge-visit work budget
and charged before graph visits. Full ontology registration, relation signatures,
version migrations and compatibility validation remain unimplemented.

Verification: ten inference unit tests and Rust-source integration pass,
including conflicting versions, independent namespaces, empty identifiers and
validation-budget exhaustion. Four-backend Query lifecycle fixtures reject
conflicting retained selections. Strict all-target inference/Query/SQL Clippy
passes. This verifies explicit bindings, not ontology semantic compatibility.
