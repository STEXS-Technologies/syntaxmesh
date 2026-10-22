# ADR-0335: Explicit inference derivation class

Status: implemented for the deterministic evaluator.

Source-of-truth section 79.3 requires distinct deterministic-rule, heuristic-rule,
model-inference, user-declared and runtime-correlation classes. Add an explicit
typed class to inferred claims. The pure path/join evaluator emits only
`DeterministicRule`: this describes the derivation, not the certainty of its
premises. Do not invent a probabilistic confidence or convert source evidence
classes into stronger evidence.

Include the class in schema-2 claim identity with a new domain separator. These
optional on-demand claims have no persisted/public wire format yet; source fact
identities and retained canonical storage do not change. Future producers must
retain their own class and witness semantics rather than reuse this evaluator
to label model output deterministic. Declaring the classes does not implement
their producers, validation, delivery or durable lifecycle.

Verification: nine inference unit tests plus real Rust-source integration pass.
The four-backend retained/retracted/restored claim fixtures pass with schema-2
identity. Strict all-target inference, Query and SQL Clippy passes. Full workspace
CI for these latest changes remains a separate gate.
