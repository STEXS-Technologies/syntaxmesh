# ADR-0341: Bounded rule-witness verification

Status: implemented for the supported deterministic rule shape.

Add a pure verifier for the supported deterministic path/join rule witness,
checking ordered exact premise references, connected nonrepeating path, join,
terminal concept, endpoints, rule/configuration/ontology metadata and schema-2
identity. Require explicit premise and depth limits. Reuse one identity encoder
in evaluation and verification without changing existing schema-2 IDs.

Query first resolves authoritative active versions using ADR-0340 and then
verifies that witness against the caller-supplied rule. This proves one supported
derivation from those premises, not completeness of a claim set, ontology
compatibility, premise certainty or authenticity of externally supplied rules.
No whole-graph inference rerun, arbitrary rule execution or durable write occurs.

Verification: twelve inference unit tests and Rust-source integration pass.
Witness tests reject forged identity, model-inference labeling, changed rule
versions, disconnected paths and exhausted limits, and compare the shared encoder
against the previous schema-2 encoding. Four-backend retained Query fixtures
verify authoritative witnesses and reject altered claim IDs. Strict all-target
inference/Query/SQL Clippy passes; recursive proof DAGs remain unimplemented.
