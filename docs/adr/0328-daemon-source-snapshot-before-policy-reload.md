# ADR-0328: Capture daemon sources before reloading preparation policy

Status: accepted for implementation; verification pending.

## Evidence and decision

The full gate intermittently leaves the verification-reload fixture serving a
Durable generation; its isolated rerun passes. The daemon callback loads project
policy before structural reconciliation scans files. For the documented operator
sequence config-on then source-edit, a running pass can capture old config-off
and newer source bytes. The next pass sees config-on but unchanged source facts;
no publication occurs and it cannot retroactively verify the durable gap.

Capture the immutable supported-source inventory first, then load and install
project policy before preparing it. Reuse SourceHost's existing
`reconcile_scanned_sources`, including durable recovery, resolver input checks
and Engine publication; do not implement another publication path. For config
written before source content, the policy read now follows source capture rather
than preceding it. Verification still applies to future publications, never
retroactively bridges an unverified generation, and its failure still stops the
daemon. Invalid configuration still fails before source publication.

This ordering is not a multi-file filesystem transaction: arbitrary concurrent
edits, replacement during reads and configuration changes during preparation
are not claimed atomic. Scan errors can now be reported before configuration
errors. No public DTO, store schema, pure-core dependency or generation format
changes. Keep the existing native and polling verification-gap fixture and full
CI as acceptance gates; an isolated passing rerun alone is insufficient.

The callback now retains pending-workflow recovery before scanning, captures the
source inventory, loads/installs policy and calls the shared scanned reconciler.
All 14 daemon lifecycle tests pass together, including native/polling verification
gap rejection, resolver reload, invalid config failure, restart and MCP transport.
Strict daemon all-target/all-feature Clippy passes. Full contributor CI and a
deterministically injected policy/scan ordering regression remain pending; this
single successful suite does not prove the intermittent failure eliminated.

The production callback now uses a focused `pass_inputs::capture` sequencing
helper. Its deterministic tests force config-on/source-revision-2 during capture
and verify the subsequent policy read selects verification; reversing the read
order would fail. A scan-error test verifies policy loading is skipped and the
original failure retained. These tests validate sequencing, not an atomic
filesystem transaction. Full contributor CI passes (exit 0) in 141.51 seconds,
including all daemon lifecycle tests and all 17 backend conformance scenarios
(42.32 seconds). Strict workspace Clippy and documentation build pass. Existing
allowed dependency warnings remain unchanged.
