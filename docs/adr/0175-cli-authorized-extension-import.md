# ADR-0175: CLI authorized extension import

- Status: accepted
- Date: 2026-09-30

## Decision

Expose `ingest-extension[-turso] <store> <grant.json> <frame> <run-id>
<generation-id>` in the existing CLI host. Require an initialized store so the
repository/worktree identity comes from its canonical manifest. Read an explicit
operator-owned grant (at most 64 KiB) and exactly one framed batch (at most 4 MiB
plus its header), rejecting trailing bytes. Accept regular files only; this
command does not wait on producer pipes or launch analyzed-source runtimes.
Decode and authorize before opening the store. Pass the batch to the existing
Penelope-backed Engine method and emit one JSON publication receipt on success.
There is no new canonical format, workflow, or NDJSON handoff.

Reuse the existing File/Turso host constructors, identifier parsing, and output
error handling. External producers may generate frame files independently; this
is a usable external import boundary, not a socket daemon or producer supervisor.
The grant must come from operator policy, never the producer's submitted manifest.
OS file access is the CLI operator boundary; cryptographic peer authentication
is not implied. A stdout failure after commit can leave an accepted generation
without a delivered receipt; clients must inspect the store before retrying.
No receipt-delivery atomicity or exactly-once retry guarantee is introduced.

The long-running host/lifecycle, production deadlines, timeout/kill supervision,
and authenticated network/IPC peer handling remain open extension work. Independent
producer exit-failure isolation is covered by the conformance fixture below.

## Verification

The subprocess fixture migrates/initializes the required stores, imports a
populated runtime observation through both File and Turso hosts, validates the
single JSON receipt, and queries the accepted historical generation after CLI
exit. Trailing frames preserve that snapshot; unauthorized grants and oversized
grant/frame inputs fail before creating an absent store. Focused strict Clippy
and `cargo make ci` pass (134.77 seconds), covering the combined framing,
producer-grant, engine-equivalence, and CLI import changes.

The existing `extension-fixture` CI task now also runs an independently built
out-of-tree producer with embedded Engine/store dependencies disabled. Its normal
dependency tree is checked for Engine, store ports/adapters, workflow/integration
adapters, and direct database/workflow dependencies. A regression test exercises
the forbidden dependency names, including Turso/SQLite and verification adapters.
The producer emits its existing catalog fact/provenance/payload and
runtime observation through the public frame codec. A second producer run exits
with an injected failure after writing eleven header bytes. File/Turso CLI imports
reject this partial output without changing graph exports or operator/workflow
diagnostics, then accept the complete frame, return `Verified`, and serve both
facts after CLI exit. StateChronicle audits report recorded verified history.
The enhanced independent-producer fixture passed the full `cargo make ci` workflow
(115.85 seconds), including all-feature tests, migration checks, dependency audits,
and documentation builds. Subsequent dependency-guard tightening has a passing
focused regression test and strict xtask Clippy check. This is independent producer
exit-failure coverage, not daemon timeout/kill supervision.
