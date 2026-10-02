# ADR-0233: Owned backend integrity read

- Status: accepted
- Date: 2026-09-30

## Decision

Expose backend integrity through a capability-bounded Engine method that delegates
to the existing `BackendIntegrityCheck` store port. No adapter or transport types
enter the Engine. GET `/api/v1/backend-integrity` uses the same admitted shared
Engine lock and owner boundary, labeling the report with the current generation.
Unknown query parameters return 400. Return the version-1 envelope with kind,
passed, and findings; failed checks remain successful report transport (200).
Bound serialized reports to 4 MiB (413 if exceeded).

Attach `integrity-turso` through existing discovery/client, validate Turso kind
and success consistency using the existing report constructor, and reuse the
embedded formatter and integrity-failure exit behavior. Active failures never
fall back to a second connection; absent ownership holds a lease through backend
inspection. This is not the logical graph-integrity check in status, and not a
cheap health probe. Client timeout does not cancel admitted blocking work.

## Verification gates

Verify output/exit equivalence on the same backend, leased fallback, malformed
kind/passed/findings rejection before output, HTTP input rejection, and full CI.

The owned live Engine fixture verifies byte-identical backend report output and
matching exit behavior against embedded inspection, plus leased fallback after
shutdown. This checks report transport equivalence, not new corruption-detection
coverage. Malformed-response fixtures reject foreign kinds and contradictory
success/findings before output with unchanged invalid database bytes. Targeted
tests and affected all-feature Clippy pass. TCP inspection matches direct Engine
reports, preserves workflow counts, and rejects unknown query parameters (400).
Full `cargo make ci` passed in 207.48 seconds, including strict workspace
all-feature Clippy, architecture/migration/dependency gates, extension isolation,
15 attachment tests, 10 HTTP tests, 16 backend-conformance tests, and API docs.
