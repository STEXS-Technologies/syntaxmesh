# ADR-0319: Source-grounded Rust test classification compatibility gate

Status: additive Rust metadata and File/Turso history verified; full CI verified.

Final shared-fixture full contributor CI passes in 133.57 seconds, including
all 17 backend/restart scenarios (42.14 seconds), the dedicated Turso role-history
target (0.24 seconds), strict workspace checks, architecture gates and docs.
Ranking consumption remains open.

Full contributor CI for the metadata/initial File fixture passes in 181.87
seconds, including all 17 backend/restart scenarios (42.66 seconds), strict
workspace checks and documentation. The fixture is subsequently shared as a
generic GraphStore + DurableRecordStore scenario, reusing the existing
cross-backend test layout. Separate all-feature File and Turso executions pass
(0.02 and 0.25 seconds); strict Engine/Turso Clippy passes. Engine gains no
database dependency. Full CI must be rerun for this final shared-fixture layout;
the earlier run is not evidence that it covered the subsequently added target.

All 33 Rust extractor tests and strict all-target/all-feature Rust Clippy pass.
The new test checks unchanged callable identity/kind, retained body-reference
ownership, unannotated helpers, and rejection of conditional/framework/lookalike
attributes. The implementation reuses `syn::Visit`, the existing declaration
map and extension payload, with one bounded node pass rather than repeated
linear searches. Nested and duplicate declaration roles preserve identities.
The existing File restart/producer-upgrade fixture now verifies unchanged-source
role addition, role removal in a third generation, unchanged callable identity,
retained earlier snapshots and StateChronicle verification after restart.
The source range includes the direct attribute, as provided by the existing AST
range compiler. Full contributor CI and broader backend role coverage remain
required; ranking consumption remains separate.

Decision: keep callable `Function` nodes and annotate explicit standard `#[test]`
intent through the existing `ExtensionPayload` slot. Namespace
`syntaxmesh.lang.rust.test-role`, schema 1, bytes `[1]` means a direct outer
path-form `#[test]` attribute was observed. Absence is unknown/not annotated,
not proof of production code. No new nodes, relations, IDs or resolver targets
are introduced. Framework attributes and conditional attributes remain unclaimed.
This is syntax evidence, not macro resolution or proof a test executes.
Advance the Rust producer to 0.16.0. Generic ranking must not decode private
Rust payloads ad hoc; a separate role-consumption contract remains necessary.

The Rust producer currently emits attributed test functions as `Function`.
`NodeKind::Test` already exists, but changing the emitted kind alone is unsafe:
`declaration_node_id` includes the kind, declaration occurrence numbering is
kind-scoped, and extraction builds its function/call ownership map by filtering
only `Function`. A classification change must preserve declaration identity,
duplicate occurrence ordering, body call ownership, resolver binding and retained
history. Increment the Rust producer version so unchanged sources are reextracted.


Before adoption, specify whether test role is represented on the existing
callable identity or as separate source-grounded classification evidence. Do not
silently repurpose a public node kind while breaking callers. Exact standard
`#[test]` attributes can establish syntactic test intent; framework attributes,
aliases and conditional `cfg_attr` need explicit, conservative policies, not
macro execution or claims of compiler-confirmed behavior.

Required evidence: ordinary helper functions in test modules remain callable;
test bodies retain their owned references; adding/removing test intent retains
the callable identity and historical versions; duplicate declarations remain
distinct; attribute lookalikes are not classified; unchanged-file reindexing
responds to the producer-version change. Ranking policy remains separate and
must still be tested with implementation, test and documentation queries.
