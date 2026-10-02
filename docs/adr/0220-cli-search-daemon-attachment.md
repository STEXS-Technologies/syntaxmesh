# ADR-0220: CLI search daemon attachment

- Status: accepted
- Date: 2026-09-30

## Decision

For `search-turso`, discover the canonical store's active owner before opening
Turso. If present, reuse `/api/v1/search` with limit 100 and the existing typed
Node representation. Send the advertised instance guard and require one matching
response header. Preserve the existing tab-separated CLI search output through
the same formatter. Validate response schema version, generation ID, and result
count, and bound response bytes to 4 MiB. Disable proxy discovery and redirects;
bound connection/request time. Do not retry attachment as embedded operation.

If no owner is observed, acquire and retain the real writer lease before opening
the embedded reader, through query/output and Engine destruction. A racing owner
causes lease acquisition to fail rather than opening a second Turso connection.
An active owner with invalid/missing discovery, transport failure, stale instance,
or invalid response is an explicit error, never automatic database fallback.

This reuses existing Rust HTTP transport and query schema. It introduces no
NDJSON storage or internal graph handoff, and no host dependency into pure crates.
Instance checks prevent accidental stale/replacement-owner use, not a malicious
listener that fabricates matching headers. Other CLI read commands and mutation
dispatch are still separate attachment work; this does not complete Phase 6.

## Verification

A real CLI process fixture uses the existing shared HTTP owner and retained Turso
Engine: attached output matches the embedded baseline byte-for-byte, publication
refreshes search, stale-instance and malformed active-owner records fail with no
stdout or canonical database changes, and release permits leased embedded fallback
despite stale metadata. Twenty host-equivalence and three CLI ownership process
tests pass. Strict CLI Clippy and architecture checks pass. This fixture composes
the same HTTP host as the daemon; it does not itself launch `syntaxmeshd` or prove
all daemon lifecycle races. Full `cargo make ci` passes for the attachment
implementation (167.20 seconds), including all-feature workspace tests,
dependency policy, migration registries, extension isolation, and API docs.

Dedicated Axum transport fixtures added afterward run real CLI processes against
an intentionally invalid database held under a writer lease. They verify proxy
bypass, correct request guards and limit, missing/wrong/duplicate response
identity rejection, disabled redirects, exact 4 MiB acceptance and oversized
response rejection, invalid schema,
and malformed JSON. Each rejected request leaves stdout empty and store bytes
unchanged rather than opening the fake database. All three attachment tests and
strict CLI Clippy pass. The full CI run predates these extra fixtures.
