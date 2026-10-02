# ADR-0176: Extension import verification opt-in

- Status: accepted
- Date: 2026-09-30

## Decision

Add an optional final `--verify` to `ingest-extension[-turso]`. Reuse Engine's
`with_statechronicle_verification`, as existing source indexing does. Verification
remains opt-in per import, with no implicit policy inferred from an external
manifest or the latest generation. The receipt reports the effective existing
workflow status; enabling verification must not change imported fact identities.
This closes a CLI host omission, not a new verification algorithm or proof type.

Reject unknown or duplicate trailing options. Input authorization still finishes
before opening the store. Runtime peer origin and trust remain unrelated to
generation verification. The grant file cannot enable or disable verification.

Reuse the existing StateChronicle integration from crates.io and the indexing
host's engine-construction pattern. No new dependency or store migration.

The existing verifier requires publication to extend the verified history head;
`--verify` is not retroactive enrollment of an unverified store. If verification
fails, the existing Engine records an accepted generation's verification failure
rather than rolling back canonical facts. CLI failure must not be interpreted as
proof that no generation was accepted. Inspect status/history before retrying.

## Verification

The File/Turso CLI import fixture now covers both modes. Verified source indexing
followed by verified extension import returns `Verified`; a reopened
`statechronicle-verify[-turso]` audit succeeds. Unverified imports retain `Durable`.
Attempting verification on an unverified base returns the existing history-head
error and no success receipt, while the accepted generation remains queryable.
All 45 CLI unit tests, the expanded four-case import fixture, strict CLI Clippy,
architecture checks, and formatting pass. A subsequent full `cargo make ci` run
passes on this opt-in implementation (113.83 seconds), including all-feature
workspace tests, backend conformance, dependency audits, and documentation builds.
The existing bincode/paste unmaintained-dependency exceptions are unchanged.
