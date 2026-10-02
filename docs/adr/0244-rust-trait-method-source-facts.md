# ADR 0244: Rust trait method source facts

Status: accepted

## Decision

Emit method declarations for required and default Rust trait functions under
their trait-qualified names. Record calls inside default bodies with the method
node as source owner. Reuse the existing `syn::TraitItem::Fn`, declaration node
identity, identifier spans, `CallVisitor`, reference occurrence IDs, and resolver
pipeline. Keep trait-specific traversal in a focused sibling module.
Advance the Rust producer to 0.7.0 so unchanged-source caches invalidate.

This fills missing deterministic language facts without introducing a new
ontology, parser, workflow, runtime, database, or public DTO.

## Limits

Required methods have no body and therefore produce no body-call occurrences.
Default-body `self` receivers remain explicit unknown receivers: there is no
concrete impl type. This does not implement trait dispatch, implementation
relationships, associated constants/types, inherited methods, compiler checks,
or cross-file Rust module resolution. Historical graphs remain unchanged.

## Verification

All 15 Rust extractor tests pass. Trait coverage verifies nested qualified names,
identifier spans for required/default methods, default-method call ownership,
and unknown `self` receiver spelling. Six shared CLI lifecycle tests pass on
File and verified Turso, including new default-body indexing, unchanged no-op
repetition, edits, and retained old graphs. The older qualified-call fixture now
uses an empty trait so adding method facts does not introduce an unrelated
same-name ambiguity into that fixture. Full `cargo make ci` passes in 223.48
seconds, covering architecture boundaries, extension isolation, migration checks,
strict Clippy without new allowances, workspace tests, all 16 backend-conformance
scenarios, and documentation generation. Existing allowed dependency policy
warnings remain unchanged.
