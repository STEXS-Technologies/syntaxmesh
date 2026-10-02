# ADR 0246: Rust implementation-block source facts

Status: accepted

## Decision

Represent each Rust impl block with a source-backed external node in the
`syntaxmesh.lang.rust` namespace, kind `implementation` or
`negative-implementation`. The file module contains the block; the block contains
its method declarations. Positive trait impls emit `Implements` occurrences
owned by the block and evidenced by the trait path. The existing trait-only
resolver determines whether the trait is indexed. Inherent and negative impls
never emit positive `Implements` references.

Names retain trait/type header spelling. IDs use file, normalized header, and
duplicate ordinal, not body text or byte offsets. Function-call ownership is
selected by declaration evidence spans within an extraction, not an overwriting
name map, so same-named methods in distinct impls keep separate call ownership.
Reuse existing external nodes, `Contains`, reference occurrences, parser spans,
and the resolver/publication/history pipeline. Advance Rust producer to 0.8.0.

## Limits

`Implements` links an impl block to a trait, not an inferred concrete type node.
Containing file and method links are structural facts. Module/type/import
resolution, compiler trait dispatch, negative semantic relations, nested local
declaration completeness, and method declaration identity redesign remain open.
Same-named candidate methods still resolve ambiguously. Historical graphs are
not rewritten.

## Verification

Extractor fixtures check positive/inherent/negative blocks, method containment,
distinct same-named method call owners, exact trait-path evidence, generic path
normalization, body/offset-independent IDs, and duplicate header separation.
Eight shared File/verified-Turso CLI fixtures cover call paths, receivers, traits,
and real impl publication/retraction with no-op repetition and retained old graphs.
Full `cargo make ci` passes in 213.81 seconds: architecture boundaries, extension
isolation, migration registries, strict Clippy without new allowances, workspace
tests, all 16 backend-conformance scenarios, and documentation generation.
Existing allowed dependency policy warnings remain unchanged.
