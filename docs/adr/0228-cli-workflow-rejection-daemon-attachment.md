# ADR-0228: CLI workflow rejection daemon attachment

- Status: accepted
- Date: 2026-09-30

## Decision

Attach `workflow-rejections-turso` through the existing owner discovery and
guarded Rust HTTP client. Reuse the existing Penelope inspection and CLI
formatter; do not open a second database connection under an active owner.
Absent ownership requires a retained writer lease through embedded inspection.
Active discovery/transport/validation errors fail without fallback.

Preserve the existing clamp to 100 items. Zero returns the existing empty page
after discovery validation, without a journal request. Deserialize strict tagged
reason DTOs privately in the CLI, then map them into existing workflow types.
Validate IDs, item count, exclusive run ordering, and any continuation matching
the last returned run on a full page before writing output. The HTTP generation
is an observation label, not historical journal selection. No NDJSON handoff.

## Verification gates

The owned live Engine fixture verifies byte-identical empty embedded/attached
output for limits 0, 1, 100, and 1000. Malformed-response fixtures pass for
over-limit counts, duplicates, reversed order, empty/incorrect continuations,
unknown reasons, and extra legacy-reason fields, with no output and unchanged
invalid database bytes. The latter exposed Serde's unit-variant unknown-field
behavior; an empty struct variant now enforces strict legacy-reason decoding.
The live-owner fixture additionally creates two real Penelope stale-base
rejections and verifies nonempty output equivalence for normal/capped limits and
explicit after-run cursors, plus post-shutdown leased embedded fallback.
Full `cargo make ci` passed in 174.28 seconds: strict workspace all-feature
Clippy, architecture and migration gates, independent extension isolation,
12 daemon-attachment tests, 10 HTTP tests, 16 backend-conformance tests, and
API documentation. Live AI quality evaluation remains opt-in and excluded.

Verify embedded/attached output equivalence, including zero, oversized requested
limits, typed rejection reasons and continuations. Reject invalid reasons, IDs,
counts, order, and cursors without output or database fallback. Reuse existing
transport identity/proxy/redirect/byte-limit checks.
