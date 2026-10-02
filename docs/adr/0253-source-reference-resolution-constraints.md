# ADR 0253: Source reference resolution constraints

Status: accepted

## Decision


Persist the non-default constraint on the canonical occurrence node using the
existing `ExtensionPayload`: namespace `syntaxmesh.reference-resolution`, schema
1, one-byte tag 1. Default Name has no payload and preserves legacy facts.
Reconstruct the constraint during unchanged-source re-resolution; reject malformed
reserved payloads instead of silently reverting to name inference. Unrelated
payload namespaces are not interpreted as this policy.

The resolver retains source spelling/evidence and an explicit unresolved node,
but emits no `Calls`/`ResolvesTo` target edge for constrained references. Resolver
revision v4 invalidates source-host no-op planning. No core/store schema,
transport, workflow, runtime, or database dependency is introduced. The SDK's
Rust struct gains a required field; downstream source producers must specify
their policy. Existing graph/history bytes remain readable.

Rust lexical producers will use this policy for known shadow bindings. This
contract alone does not implement lexical target binding, Rust modules, compiler
dispatch, or the remaining product gates.

## Verification

The all-feature workspace check passes. All eight language-SDK tests and twelve
resolver tests pass, including reserved-payload rejection, legacy/foreign
metadata compatibility, and constrained resolution preserving evidence without
inventing a target edge. Unchanged-source reconstruction now decodes the stored
constraint and rejects malformed reserved metadata.

Rust shadow-binding production is implemented in ADR-0254. File and verified
Turso process lifecycle tests pass: definition edits re-resolve an unchanged
caller without losing constrained occurrence bytes/provenance, inventing target
edges, or changing retained history; repeated scans remain cached. Full CI passes
in 230.15 seconds, including workspace strict Clippy, all 16 backend-conformance
scenarios, independent extensions, migration registries, and documentation.

Additional indexer regression: unknown reserved schema, empty tag, and invalid
tag are seeded as canonical stored occurrence metadata. Editing a matching
definition triggers unchanged-source reconstruction, which returns the exact
integrity error before publication; the retained snapshot and generation journal
remain unchanged. All 13 indexer tests and strict indexer Clippy pass. This
negative-path fixture initially used InMemory. It now also reopens a real File
snapshot before the definition edit and checks unchanged snapshot bytes,
retained facts, and generation journal after rejection. All 14 indexer tests and
strict indexer Clippy pass. Full CI including this regression and the test-only
retrieval diagnostics passes in 218.04 seconds, including all 16 backend
conformance scenarios. This is not malformed-payload Turso restart evidence.
