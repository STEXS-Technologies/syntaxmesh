# ADR 0278: Feature-additive temporal page construction

Status: accepted, 2026-10-01

Enabling Query's benchmark instrumentation while leaving Turso's instrumentation
disabled fails compilation: Cargo enables instrumented store-page fields, but
the adapter's own cfg omits their initializers. Default and whole-workspace
all-feature builds miss this mixed-feature failure. SQLite and external test
adapters have the same literal-construction pattern.

Add pure store-port constructors for HistoricalEdgePage and ConsequenceRangePage
that initialize their optional fields in the crate which owns those fields.
Instrumented adapters attach their counters through feature-gated builder methods;
uninstrumented adapters use the constructors without knowing the store's feature
set. Zero default counters are unmeasured, not proof of zero storage work.
Reuse Rust's additive feature and builder patterns; do not add a runtime, change
canonical storage or make instrumentation mandatory. Update built-in durable
adapters and the external-style Query test adapter. Add mixed-feature compilation
to contributor verification so downstream consumers can choose features safely.

Both downstream-style mixed checks pass, as does the previously failing combined
Query/MCP all-feature Clippy invocation. Default Store Clippy and the constructor
test with instrumentation enabled pass. Constructors use const zero initialization
and the test compares it with the existing metrics defaults before attaching
measured counters. Full contributor verification passes in 282.67 seconds,
including both mixed-feature checks, strict workspace Clippy, dependency policy,
all 16 backend conformance scenarios, workspace/doc tests and API documentation.
