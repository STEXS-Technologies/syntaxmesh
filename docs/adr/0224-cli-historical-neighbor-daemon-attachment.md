# ADR-0224: CLI historical neighbor daemon attachment

- Status: accepted
- Date: 2026-09-30

## Decision

Attach `neighbors-at-turso` through the existing guarded HTTP neighbor route.
Keep explicit generation, direction, edge continuation, 1–1000 item CLI limits,
and the existing temporal export records. Compose HTTP pages of at most 100 up
to the requested limit; emit one CLI footer through the shared query formatter.
Validate all pages before output. Active-owner failures never reopen the database;
no-owner fallback retains a writer lease.

Move the existing version-1 base64url JSON cursor codec into the public
interchange crate, without changing its bytes or validation. Base64 is a pure
encoding dependency, not a transport/runtime/host dependency. Both hosts reuse
it; the CLI validates decoded continuations against generation, endpoint,
direction and last edge before using them. No new endpoint or opaque format is
invented and NDJSON remains export only.

An initial guarded 404 may mean absent seed or generation. Confirm the selected
generation through the existing generation-selected search route before
constructing an empty terminal page, preserving embedded missing-seed semantics
without treating unknown generations as empty. A later 404 fails closed.

## Verification

The real CLI/shared-host fixture verifies byte-equivalent exports for incoming
and outgoing queries, limits 1/101/1000, typed edge continuation, missing seeds,
retained history after publication, and leased fallback after owner shutdown.
Unknown generations fail with no output. Adversarial fixtures reject malformed
tokens, foreign cursor generation/endpoint/direction/edge coordinates, and
changed continuation-page generations without partial output or database opens.
Existing cursor round-trip/rejection and HTTP host tests pass.

Full `cargo make ci` passes in 200.26 seconds, including eight attachment tests,
all-feature workspace tests, strict Clippy, migration registries, out-of-tree
extension isolation, architecture checks, dependency policy, and API docs.
Broader daemon lifecycle and remaining CLI attachment work remain open.

## Client lifetime follow-up

Attachment now retains one blocking HTTP client for a complete command, including
current/historical neighbor continuations and historical missing-seed checks.
This adapts Shardline's `sdx` client builder/retained-transfer-client pattern;
SyntaxMesh retains its own proxy bypass, disabled redirects, two-second connect
and five-second per-request timeout, owner-instance checks, and 4 MiB response
bound. No public wire contract or query policy changes.

A loopback fixture observes two successful requests using the same TCP peer,
verifying actual connection reuse rather than merely one client constructor.
That fixture, all eight attachment integration tests, strict CLI Clippy,
formatting, and architecture checks pass. The full CI result above predates this
client-lifetime refactoring; no latency improvement is claimed without a benchmark.

An additional continuation fixture changes only the owner response headers on
page two: missing, wrong, and duplicate identities are rejected by both current
and historical CLI collectors before any output. The shared fixture now also
requires the exact planned request count. Nine attachment tests, strict CLI
Clippy, formatting, and architecture checks pass for this follow-up; production
identity validation needed no change.
