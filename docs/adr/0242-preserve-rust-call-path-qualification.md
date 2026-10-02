# ADR 0242: Preserve Rust call-path qualification

Status: accepted

## Decision


This feeds the existing conservative resolver and occurrence/history model;
it introduces no new DTO, runtime, parser, database, or resolution algorithm.

## Limits

This prevents qualified calls from masquerading as unrelated bare functions.
It does not implement trait dispatch, inherent qualified-self lookup, external
crate lookup, visibility, macro expansion, or compiler type inference. Retained
historical generations are not rewritten.

## Verification

All 13 Rust extractor tests pass, including exact target spelling and complete
source spans for bare, segmented, trait-qualified, inherent-qualified-self, and
absolute paths. Two real CLI lifecycle fixtures pass on File and verified Turso:
three unsupported qualified occurrences remain unresolved, ordinary bare and
segmented calls resolve, unchanged indexing is cached, edits resolve each
occurrence without changing the retained original graph. Direct semantic edges
still collapse repeated calls to the same target; occurrence nodes retain each
site. Full `cargo make ci` passes in 219.95 seconds: architecture boundaries,
extension isolation, migration registry checks, strict Clippy, workspace tests,
all 16 backend-conformance scenarios, and documentation generation. Existing
allowed dependency audit/policy warnings remain unchanged.
