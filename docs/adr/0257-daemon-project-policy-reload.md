# ADR 0257: Daemon project policy reload

Status: accepted

Load the existing project configuration before every watch reconciliation.
Compare effective resolver profiles/source roots and verification policy.
Construct changed providers before acquiring the engine lock, then replace
engine policy and planning fingerprint together under that lock. Reuse existing
config parsing, provider construction, and reconciliation; no new parser or
second database connection. Configuration parse/provider errors fail the watch
and stop serving, consistent with current fail-closed watch errors.

CLI `--verify` remains a positive override. Verification-only changes affect
future publications and do not retroactively verify history or force a no-op
generation. Resolver fingerprint changes trigger existing structural planning.
Config deletion restores defaults. Periodic reconciliation guarantees detection
even when native notifications do not trigger a source pass. No guarantee of
immediate reload, semantic-provider reload, bounded shutdown, or concurrent
configuration editing is introduced.

Compilation and strict all-target daemon Clippy pass. A process fixture covers
native and polling modes: enabling a resolver publishes a distinct generation
without restarting the owner, config deletion restores defaults through another
transition, original historical source remains readable, and malformed TOML
exits unsuccessfully and releases ownership. The expanded process fixture also
checks unchanged TypeScript import facts: Node resolver enable adds a ResolvesTo
edge, config deletion removes it, and the configured historical generation
retains it. Both native and polling modes pass. The configured reload explicitly
sets project verification false while CLI `--verify` stays active; after shutdown
the existing StateChronicle audit verifies retained history. Disabling/enabling
verification without the CLI override now has a native/polling process regression:
project true starts Verified; project false plus a source edit publishes Durable;
reenabling plus another edit publishes canonically but fails verification because
its parent does not extend the verified head. The process exits unsuccessfully,
current status is VerificationFailed, the three-generation lineage is retained,
and the earlier StateChronicle chain still audits. Historical canonical manifests
remain Durable by design; they are not the per-generation verification records.
The targeted regression and strict daemon Clippy pass. Full CI passed in 216.47
seconds, including strict workspace Clippy, all 11 daemon process tests, and all
16 backend-conformance scenarios, before this added transition fixture. A new
full gate now passes in 171.06 seconds, including all 12 daemon process tests,
strict workspace Clippy, and all 16 backend-conformance scenarios.
