# ADR-0229: Shared inventory freshness comparison

- Status: accepted
- Date: 2026-09-30

## Decision

Expose `EngineIndexFreshness::compare(indexed, source)` as a pure inventory
comparison. Move the existing Engine algorithm into a focused sibling module;
`SyntaxMeshEngine::index_freshness` reads current indexed files and delegates.
No filesystem, runtime, transport, or database access occurs in the comparison.
Callers remain responsible for matching repository/worktree scope.

Preserve file-ID matching and full FileVersion equality, including normalized
path, content hash, and size. Repeated IDs preserve the existing last-entry-wins
map behavior; this helper does not validate inventories. Scope and duplicate
validation belong to a transport boundary when accepting external inventories.

This supports future CLI status attachment without duplicating freshness rules.
It does not implement that attachment, cache logical integrity, or establish an
atomic relationship between a host source scan and a generation read.

## Verification

Check unchanged, added, changed, removed, reordered, and duplicate inventories.
Existing Engine freshness tests must continue passing through delegation.

The two pure-comparison tests and existing Engine added/changed/removed inventory
test pass through this implementation. Strict Engine Clippy passes. Status
attachment remains follow-up work; no attached status behavior is claimed here.
