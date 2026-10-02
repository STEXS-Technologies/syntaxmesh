# ADR 0241: Preserve unmatched qualified references

Status: accepted

## Decision

After exact normalized lookup, a reference containing `::` remains unresolved
instead of falling back to a repository-wide terminal-name match. Unqualified
unique-name lookup and exact generic-normalized matches remain unchanged.
Reuse the existing reference occurrence, resolution edges, and invalidation
pipeline. Export a static resolver revision marker and include it in shared
CLI/daemon source-planning fingerprints, even when optional module providers are
disabled, so unchanged sources are re-resolved after upgrading.

## Limits

This prevents invented qualified bindings; it does not implement lexical scope,
Rust `crate`/`self`/`super` path resolution, imports, traits, or compiler semantics.
Embedded hosts with custom no-op planning must include the revision in their own
configuration identity. Retained historical facts are not rewritten.

## Verification

Resolver regression coverage requires qualified misses (including generic paths)
to remain unresolved, while exact and unique unqualified calls still resolve.
Source-host coverage verifies that the revision republishes unchanged sources
once, then returns to no-op reconciliation.

All 20 CLI host-equivalence tests pass. Their shared Rust fixture now explicitly
uses supported unqualified matching rather than claiming unsupported cross-file
`crate` path resolution. Embedded generation identity reuses the shared source
inventory packing and resolver configuration instead of duplicating them.
The live HTTP watch fixture also includes the resolver fingerprint in its initial
publication, matching subsequent reconciliation and preserving its strict
source-edit assertion. Full `cargo make ci` passes in 191.09 seconds, including
strict Clippy, architecture and migration checks, extension isolation, workspace
tests, all 16 backend-conformance scenarios, and documentation generation.
Dependency audit/policy checks retain their existing allowed warnings.
