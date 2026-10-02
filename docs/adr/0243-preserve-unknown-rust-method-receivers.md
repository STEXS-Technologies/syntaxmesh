# ADR 0243: Preserve unknown Rust method receivers

Status: accepted

## Decision

Apply the existing Python extractor policy to Rust: only a syntactic `self`
receiver with an enclosing impl type yields an impl-qualified method target.
Keep other receivers in the authored target spelling rather than replacing them
with a bare method name. Reuse `syn` expression variants and `quote::ToTokens`;
parentheses, groups, and reference wrappers around `self` are transparent.
Method-call evidence covers the full expression, including receiver and arguments.
Advance the Rust producer to 0.6.0 to invalidate extraction/configuration caches.

The existing resolver, occurrence identities, publication workflow, and retained
history remain the implementation boundaries. No DTO or runtime is added.

## Limits

Unknown receiver types remain unresolved; this is not receiver inference, trait
dispatch, dereference analysis, or compiler semantics. A syntactic `self` match
retains the existing impl-name approximation. Historical generations are not
rewritten.

## Verification

All 14 Rust extractor tests pass. Receiver coverage checks plain, parenthesized,
and reference-wrapped `self`, unknown receivers, and `self` without impl context,
including exact source spans. Four shared CLI call-path/receiver fixtures pass
on File and verified Turso, checking unchanged no-op indexing, edit-time resolution
and retained old graphs. Unknown receivers stay unresolved despite an indexed
same-named free function. Full `cargo make ci` passes in 224.68 seconds, including
strict Clippy without new allowances, architecture boundaries, extension
isolation, migration checks, workspace tests, all 16 backend-conformance
scenarios, and documentation generation. Existing allowed dependency policy
warnings are unchanged.
