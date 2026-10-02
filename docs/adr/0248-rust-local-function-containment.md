# ADR 0248: Rust local function containment

Status: accepted

## Decision

Reuse the shallow local-item visitor, declaration-span lookup, and existing
`Contains` edge convention to connect local function declarations directly to
their containing function, impl method, or default trait method. Walk nested
functions independently so deeper declarations attach to their immediate owner.
Advance the Rust producer to 0.10.0 to invalidate cached extraction.

This adds source facts, not lexical target binding. Functions inside local
modules, local impls, or local traits are not falsely attached directly to an
outer function. Their broader declaration containment remains separate work.
No new public DTO, dependency, storage schema, or runtime is introduced.

## Verification

All 21 Rust extractor tests and extractor strict Clippy pass. The new fixture
checks immediate multi-level containment, impl/default-trait bodies, provenance,
and rejection of flattened local-module containment. The existing 10 shared CLI
File/verified-Turso lifecycle tests now check local containment in both current
and retained generations after edits, no-op repetition, and restart.
Full `cargo make ci` passes in 215.88 seconds, including workspace strict Clippy,
architecture and migration-registry checks, extension isolation, all 16
backend-conformance scenarios, and API documentation. Existing allowed
dependency advisories remain unchanged.
