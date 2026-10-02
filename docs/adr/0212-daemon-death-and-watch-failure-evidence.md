# ADR-0212: Daemon death and watch failure evidence

Reuse the existing CLI child-process death and safe cleanup fixture pattern for
the executable daemon. Test abrupt death after a committed generation, then
restart against the same persistent sidecar and database. Verify current/history
retention, an edit made while stopped being reconciled, and the complete
StateChronicle history after shutdown. Never delete the ownership sidecar.

Also force a deterministic source-decoding failure during polling reconciliation.
The daemon must fail, release ownership, and restart after the input is repaired
without duplicating an unchanged generation. This tests the host's failure path,
not just the reusable watch loop. It does not establish crash atomicity at every
instruction inside publication: injected mid-publication process death and IPC
socket lifecycle coverage remain open.

Verified: all five daemon lifecycle fixtures pass, alongside strict daemon
Clippy and architecture checks. Abrupt-death coverage runs native and polling
modes, audits StateChronicle before and after restart, and retains old search
while publishing offline edits. Source-decoding failure exits nonzero; repair
retains the existing generation. Full workspace CI passes with these fixtures
and the shared MCP host change (155.10 seconds); ADR-0213 records the run's scope
and remaining opt-in evaluations.
