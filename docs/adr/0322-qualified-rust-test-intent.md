# ADR-0322: Source-grounded qualified Rust test intent

Status: implementation, focused verification and full contributor CI pass.

Full contributor CI passes in 179.32 seconds, including all 17 backend/restart
scenarios, dedicated File/Turso role history, strict workspace checks,
architecture/extension/migration gates and documentation. Generic role-aware
context ordering and representative quality evidence remain open.

All 34 Rust tests and strict Rust Clippy pass, including positive qualified
path/list cases and negative name-value, conditional, global-qualifier,
lookalike and brace/bracket cases. Both shared File and Turso history targets
pass with direct qualified syntax in the upgrade/removal scenario; legacy
0.16.0 standard-test metadata remains separately covered. Strict Engine/Turso
Clippy passes. Full contributor CI remains required; no relevance claim follows.

Reuse the current `syn` attribute visitor and SDK role codec to recognize direct
outer `tokio::test` and `async_std::test` paths, in path or parenthesized-list
form. The two segments must match exactly with no leading global qualifier or
generic path arguments. Standard `#[test]` retains its path-only rule.
Record TestIntent, not compiler-confirmed resolution, macro validity or execution.
Aliases, conditional `cfg_attr`, name-value forms, unrelated qualified paths,
and function/directory-name guesses remain unknown. No analyzed runtime launches.

This covers the direct Tokio attributes observed in Shardline's crowded webhook
tests. It does not make missing metadata proof of production code or establish
relevance improvements. Advance the Rust producer to 0.18.0; preserve callable
identities and use the shared SDK encoding. Reuse File/Turso history fixtures
with qualified test syntax before changing ranking.
