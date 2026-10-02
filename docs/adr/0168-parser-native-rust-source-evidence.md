# ADR-0168: Parser-native Rust source evidence

- Status: accepted
- Date: 2026-09-30

## Decision


Move the existing substantial Rust unit suite into `extractor/tests.rs`, following
the project's existing Shardline-derived test layout. Preserve original scenarios.

## Evidence

Before the fix, regression tests demonstrate a declaration span pointing at a
comment (byte 3 instead of 56) and a Unicode import span selecting `le::Él`
instead of `Élément`. Verify comments/earlier calls, multibyte source, raw
identifiers, parser-stripped prefixes, stable identities, and retained history.

All 12 Rust extractor unit tests pass, including the three new source-range
regressions and nine preserved scenarios. Strict Clippy and combined contributor
checks pass (`cargo make ci`, 109.27 seconds). Existing backend/history conformance
also passes; no dedicated live Rust producer-upgrade history claim is made here.
