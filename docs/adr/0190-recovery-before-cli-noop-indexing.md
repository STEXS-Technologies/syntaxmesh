# ADR-0190: Recover before CLI no-op indexing

Status: Accepted

## Decision

Run Engine::recover_pending_workflows before scanning or deciding source-index
fingerprint reuse on every CLI index/watch pass. Reuse existing Penelope
publication reconciliation rather than creating a second host recovery workflow.
Recovery failures stop the pass without claiming source freshness or updating
the CLI source marker. Resolved generations are read only after reconciliation.

Expose consuming Engine::into_store, mirroring Indexer::into_parts and
PenelopeWorkflow::into_store. Host setup can recover an opened store through a
temporary Engine then construct the configured extraction Engine on that same
store, without reopening or adding host dependencies to public/core contracts.
The consumed Engine's extractor/resolver/clock/verification configuration is
discarded; this transfers the store, not a configured Engine session.

This covers durable publication workflows supported by the existing recovery
method. Semantic provider jobs still require their provider and the semantic
execution path; analytics workflows remain separate. It does not establish
complete recovery of every service or power-loss atomicity.

## Verification

A real subprocess fixture indexes and verifies a baseline, inserts an invalid
publication record, then confirms unchanged index and watch passes fail without
success output on File and Turso. Published facts remain queryable afterwards.
The Engine store-transfer test preserves generation and completed workflows;
the existing Penelope prepared-publication regression recovers before and after
graph commit. Those tests, the 20 host-equivalence tests, watch integration,
strict Engine/CLI Clippy and the architecture gate pass. The invalid-record
fixture proves the host gate, not a process-kill or power-loss guarantee.
Full `cargo make ci` passed in 151.41 seconds after lifecycle fixtures were added.
