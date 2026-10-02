# ADR-0222: CLI current neighbor daemon attachment

- Status: accepted
- Date: 2026-09-30

## Decision

Attach `neighbors-turso` through the existing guarded HTTP historical-neighbor
pages. Request outgoing pages of 100, pin the first returned generation, and
send it with every continuation under the same owner instance. Validate page
endpoint/direction, generation continuity, count, edge ordering and incidence,
neighbor identity, and bounded non-repeating opaque cursors. Do not truncate at
the first page or let concurrent publication change the selected graph.

Collect and validate the complete result before output, matching the existing
embedded vector-returning query; reuse one tab-separated formatter. A guarded
404 on the initial page produces empty output, preserving embedded missing-seed
behavior. A missing continuation or any inconsistent page is an error with no
partial output or embedded retry. This assumes the honest scoped server's 404
contract; instance identity does not authenticate a malicious listener.

No-owner fallback retains a real writer lease. Reuse the existing guarded client,
HTTP page route, and typed Edge/Node schemas. Page responses retain the 4 MiB
bound; whole-neighborhood memory/output remains degree-dependent, as embedded
operation already is. Historical neighbor export and other commands remain open.

## Verification

The real CLI/shared-owner fixture indexes a Rust caller with 120 targets and
requires more than 100 embedded neighbor lines. Attached output matches all
baseline bytes across HTTP pages; a missing seed succeeds with empty output.
All seven attachment tests, twenty host-equivalence tests, and three ownership
process tests pass. Strict CLI Clippy and architecture checks pass. Dedicated
malformed-page fixtures now reject foreign endpoint/direction, invalid incidence
or neighbor identity, unordered edges, excessive item count, empty nonterminal
pages, and empty/oversized cursors. Two-page fixtures verify pinned-generation
request parameters and reject changed response generations, repeated edges, and
repeating cursors, with no partial stdout or fake database changes.

Full `cargo make ci` passes for neighbor attachment and malformed-page fixtures
(159.15 seconds), including all-feature workspace tests, migration registries,
extension isolation, strict Clippy, dependency policy, and API documentation.

A subsequently added Rust test proxy obtains the first page from the actual
shared HTTP host, publishes edited sources through the retained Engine before
returning it, then forwards continuations to that same host. The real CLI returns
all original neighbor bytes despite removal of the old caller from the new
current graph. Assertions prove a different generation and positive new-source
search, so publication is not merely simulated by response labels. This fixture
passes separately; the full CI run predates it.
