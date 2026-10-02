# ADR-0320: Reuse the SDK metadata codec pattern for source roles

Status: additive contract and full contributor CI verified; producer/query integration pending.

Full contributor CI passes in 217.27 seconds, including all 17 backend/restart
scenarios (40.15 seconds), dedicated Turso role history, strict workspace Clippy,
architecture/extension/migration checks and documentation. Producer output and
ranking are unchanged.

All nine SDK unit tests and strict all-target/all-feature SDK Clippy pass.
Codec coverage includes canonical and legacy decoding, exact schema/byte checks,
empty/unknown/trailing bytes, unsupported versions, and opaque unrelated payloads.
Producer adoption, generic query consumption and full contributor CI remain open.

Reuse `ReferenceResolution`'s pure, versioned extension-payload codec pattern
for `SourceRole`: `Unknown` or `TestIntent`. Unknown means no applicable positive
evidence, never production code. The canonical namespace is
`syntaxmesh.source-role`, schema 1, bytes `[1]` for test intent. Unknown emits no
payload. Accept ADR-0319's already emitted Rust namespace as a compatibility
input for the same exact schema/bytes; reject malformed recognized metadata.
Unrelated extension namespaces remain opaque and yield Unknown.

The SDK owns codec interpretation so generic consumers need not depend on a
Rust extractor or decode private bytes. This adds no node/DTO fields, changes
no serialized enum ordinals or identities, and does not change Rust producer
output or ranking yet. Other languages must provide positive source-grounded
evidence before emitting the canonical role; names/directories are insufficient.
