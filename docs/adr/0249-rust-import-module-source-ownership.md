# ADR 0249: Rust import module source ownership

Status: accepted

## Decision

Bind Rust inline-module import owners by declaration evidence spans, reusing
the span-to-node lookup already used for function call ownership. Do not
reconstruct module identities in a second visitor from module-only paths and
independent occurrence counters. Local modules inside function, impl-method,
and default trait-method bodies have containing-declaration-qualified identities.
The old reconstruction can reference nonexistent nodes or the wrong same-named
module. Missing inline-module declarations fail extraction rather than emitting
an invented owner. Non-module local imports retain their existing module owner.

Advance Rust producer version to 0.11.0. No DTO, dependency, storage schema, or
module-resolution strategy changes. Lexical import scope remains separate work.

## Verification

All 22 extractor tests and extractor strict Clippy pass. The source-ownership
fixture distinguishes same-named root/function/impl/default-trait modules; the
existing duplicate inline-module test also passes. All 12 shared CLI lifecycle
tests pass, including new File/verified-Turso local-module import edit, no-op,
restart, and retained-history scenarios. Full `cargo make ci` passes in 290.66
seconds, including workspace strict Clippy, architecture and migration-registry
checks, extension isolation, all 16 backend-conformance scenarios, and API
documentation. Existing allowed dependency advisories remain unchanged.
