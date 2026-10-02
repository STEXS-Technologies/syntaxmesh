# ADR-0321: Adopt the existing shared source-role codec in Rust

Status: producer adoption, compatibility and full contributor CI verified.

Full contributor CI passes in 182.91 seconds, including all 17 backend/restart
scenarios (42.20 seconds), both dedicated Turso role-history scenarios,
strict workspace checks, architecture gates and documentation. The codec adoption
is verified; qualified-framework coverage and ranking consumption remain open.

All 33 Rust extractor tests, both shared File history scenarios and both Turso
history scenarios pass. The legacy 0.16.0 role upgrades on unchanged source,
retains the earlier snapshot/namespace, preserves callable identity, and decodes
both generations as TestIntent after restart with StateChronicle verification.
Strict all-target/all-feature Rust/Engine/Turso Clippy passes. Full contributor
CI remains required; ranking is unchanged.

Committed-Shardline source inspection finds 7,455 direct standard test-attribute
lines and 2,632 direct Tokio test-attribute lines (text counts, not AST or runtime
coverage measurements). The webhook test excerpts responsible for small-budget
crowding use Tokio attributes. The current standard-only policy intentionally
leaves those roles unknown, so adopting the codec alone cannot solve that case.
An explicit qualified-framework syntactic-intent policy is needed before
assuming role-aware ranking can distinguish those tests. Do not replace missing
evidence with test-directory or function-name guesses; no framework is executed.

Rust reuses `SourceRole::TestIntent.payload()` instead of constructing private
bytes. Advance its producer version from 0.16.0 to 0.17.0 so unchanged source
is reextracted. Callable identities, node kinds, attribute policy, body ownership
and ranking remain unchanged. ADR-0320 decodes retained 0.16.0 Rust payloads.

Reuse the shared File/Turso upgrade fixture to verify a 0.16.0 legacy-role
generation is retained after a 0.17.0 canonical-role publication and restart.
Both versions must decode to the same positive intent with the same callable
identity, without modifying old metadata or bridging verification gaps.
