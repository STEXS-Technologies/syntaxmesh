# ADR-0126: Fuzz the supported source extractors in an isolated nightly package

- Status: accepted
- Date: 2026-09-29

## Context

SyntaxMesh accepts untrusted Rust, TypeScript/JavaScript, Python, Bash, and
documentation text. Their parsers already have deterministic unit fixtures and
the project has representative-repository integration evidence, but no
coverage-guided harness checks that arbitrary malformed source remains
fail-closed and panic-free. Shardline keeps libFuzzer targets in a dedicated,
non-product package and runs longer campaigns separately from normal stable
CI.

## Decision

1. Add a `crates/fuzz` package excluded from the stable workspace. It may use
   the nightly-only `cargo-fuzz` toolchain, but product crates and `cargo make
   ci` gain no fuzzing dependency or runtime requirement.
2. Start with one source-extractor target that chooses among the in-scope
   extractors, converts bounded arbitrary bytes to UTF-8 source, and invokes
   the selected Rust implementation without launching any analyzed runtime.
   Fuzzing must not bypass extractor errors or publish partial results.
3. Seed the target with small representative inputs, including TypeScript
   namespace imports and document blocks. Keep generated artifacts/crashes
   outside tracked fixtures; minimized crashers become ordinary regression
   tests before any fix is considered complete.
4. Expose a bounded local smoke task. Keep extended fuzz campaigns separate
   from required stable CI until runtime and resource costs are characterized.

## Consequences

- Adversarial parser testing becomes reproducible without changing supported
  language, extraction, storage, or public DTO contracts.
- One combined target provides broad first coverage; parser-specific corpora
  and semantic invariants can be split into dedicated targets when evidence
  justifies the additional maintenance.
- Coverage-guided fuzzing complements curated tests and full-corpus runs; it
  does not establish parser completeness or language-specification fidelity.

## Verification

- Build the isolated target with the installed nightly toolchain.
- Run a bounded smoke campaign over checked-in seed inputs and verify it exits
  without crashes.
- Confirm `cargo make ci` and the production architecture dependency check do
  not include the fuzz-only package or `libfuzzer-sys`.
- Local evidence (2026-09-29): the corrected seed-selector harness completed
  459,549 executions during a 20-second campaign, with no crash reported by
  libFuzzer/AddressSanitizer. The installed toolchain was rustc
  `1.99.0-nightly (7608eb7b0 2026-08-05)` with cargo-fuzz 0.13.2. See the v0
  plan for throughput/resource context and the limited scope of this evidence.
- `cargo make ci` passed locally on Linux after the harness changes, including
  excluded-target formatting, architecture boundaries, strict Clippy, backend
  migrations, workspace tests, dependency checks, and API documentation.

## References

- [Shardline fuzz package](https://github.com/STEXS-Technologies/shardline/tree/main/crates/fuzz)
- [Shardline reliability fuzz workflow](https://github.com/STEXS-Technologies/shardline/blob/main/.github/workflows/reliability-fuzz.yml)
