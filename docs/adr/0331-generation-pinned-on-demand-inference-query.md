# ADR-0331: Generation-pinned on-demand inference

Page validation now rejects payload/fact identity mismatches, zero/future sequence
positions, inconsistent or invalid validity intervals, descending/duplicate
entries, oversized pages and foreign/empty/repeated continuations. Three Query
inference tests (including the mutation matrix) and both SQL lifecycle tests
pass after this hardening; strict Query Clippy passes. Full accumulated CI is
pending. The store remains authoritative for generation-ID/sequence mapping;
these tests validate adapter contracts, not arbitrary corrupt-backend recovery.

Shared SQL lifecycle verification passes on SQLite and Turso after explicit
migration, retraction and reopen. The same fixture used by reference tests
compares complete retained claims and earlier premise-version witnesses;
current results remain empty and source calls remain intact. Four backend
lifecycle tests, strict Query/Turso Clippy and architecture checks pass.
Malformed-page fixtures and full adapter CI remain open. SQL adapter test
dependencies opt into inference; normal production consumers remain unchanged.

Status: implementation in progress; durable lifecycle verification open.

InMemory and File lifecycle tests now pass. Calls introduced in an earlier
generation retain that valid-from version while two later declaration premises
retain theirs. Retraction removes current claims but preserves historical claims
and canonical call facts. File reopen preserves the entire retained claim and
identity. Zero/exhausted history budgets reject. Strict all-target/all-feature
Query Clippy passes. SQL-backed lifecycle and malformed-page fixtures, and full
CI for this adapter, remain open; no incremental materialization is implemented.

The optional Query adapter compiles and passes strict all-target/all-feature
Query Clippy. No lifecycle completion is claimed yet; authoritative retained
selection and reopen fixtures remain the next required gate.

Query's optional `inference` feature composes the pure evaluator with the existing
pinned fact-history feed. Active entries have no valid-until sequence known at
that selection; their actual valid-from IDs identify premises. Never assign the
current generation to every edge or compare opaque generation IDs as timestamps.

The `infer(rule, limits, history_scan_budget)` contract has a separate positive
history scan budget because closed versions cost work too. Validate page size,
ordering and continuation before accepting results. Exhaustion rejects rather
than returning partial claims. This is complete bounded selection evaluation,
not constant-time lookup or a guarantee that reference backends scan internally
in bounded time. No publication, AI, host runtime or canonical mutation occurs.

Verify current/retained results after premise retraction and durable reopen,
exact old-version witnesses, source-fact preservation and malformed-page/work
failure. Penelope materialization and incremental truth maintenance remain future
work. Structural-only consumers do not enable this feature by default.
