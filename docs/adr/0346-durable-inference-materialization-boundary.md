# ADR-0346: Durable inference materialization boundary

Status: initial complete-selection storage and Penelope rebuild recovery
implemented. Direct premise lifecycle and opt-in Penelope recovery are added
under ADR-0347; newly enabled claim discovery and host delivery remain pending.

## Penelope rebuild coordinator contract

The coordinator is an opt-in `inference-sqlite` feature of the existing
Penelope integration. Journal identity binds semantic materialization identity
and absolute destination; the request digest additionally binds exact execution
controls. Prepared requests are immutable and recovered with their recorded
bounds, matching the existing analytical materializer's approach. Changing
bounds does not create a new semantic materialization and must not silently
replace a prepared request. A later explicit request-reconfiguration contract
is needed for enlarging failed bounds.

Persist a replay-validated Penelope start before opening the effect database and
persist completion only after atomic effect publication. A crash between those
steps retries the idempotent rebuild. Completed retries validate the retained
receipt against bounded downstream reads without canonical fact/rule replay;
missing or inconsistent downstream data must not yield false success. Recovery
uses bounded record pages and rejects corrupt/key-substituted records. No default
engine feature, scheduler or second workflow runtime is introduced.

Source-of-truth sections 79.4 and 80 require materialized derivations and retained
claim lifecycle, not only on-demand evaluation. The next implementation must use
the existing Penelope prepared-request/effect/completion/recovery pattern from
`syntaxmesh-integration-penelope`'s analytics materializer. Do not introduce a
second scheduler, embed workflow dependencies into the pure inference crate, or
publish derived claims as canonical extracted edges.

## Required effect contract

The separate downstream materializer owns schema migrations and an atomic
generation watermark. Persist complete claims with rule/configuration/ontology
identity, derivation class and exact ordered premise versions. Persist dependency
postings and validity intervals in the same transaction as claims and watermark.
Idempotent request identity binds repository, worktree, source generation and
complete rule-set configuration; work limits are execution controls, not alternate
semantic identities. A failed or budget-exhausted evaluation publishes no complete
watermark and preserves the last accepted materialization.

Initial rebuild evaluates a pinned canonical selection. Incremental invalidation
uses the accepted-generation feed and exact postings already implemented under
ADR-0344/0345. Closed premises end claim validity; retain the claim and reasons
for historical explanation. Restoration of a fact identity does not revive its
old derivation. Re-evaluation/new-claim discovery is required separately: merely
closing old claims is not a complete incremental synchronization algorithm.

## Recovery and verification gates

Journal scoped requests before effects using Penelope; replay validates request
and operation identities before retry. Recover partial work through the effect's
transactional cursor, not by treating an incomplete claim set as complete.
Required fixtures cover interrupted effects, journal corruption, retries,
reopening, rollback, same-identity restoration and equivalence with a fresh
bounded rebuild. Optional StateChronicle graph verification remains a canonical
publication concern; no derived watermark may impersonate its proof.

The opt-in `syntaxmesh-inference-sqlite` adapter now provides initial durable
complete-selection storage. Explicit migration bootstraps a separate database
through an ordered named SQL registry and checksum ledger; ordinary open never
creates or upgrades a database. Unrelated databases and mismatched ledgers reject.
Schema v1 retains immutable operation-scoped claim sets, binary bincode claim
payloads, exact ordered premise postings and a configuration-scoped latest
watermark. Pure claims gain Serde support for the adapter; this is not an NDJSON
storage format or a public transport contract.

Rebuild evaluates a pinned canonical selection before the write transaction.
Encoded output and premise admission must succeed before publication. Claims,
postings, operation receipt and watermark commit atomically; historical rebuilds
retain their own selections without regressing the latest watermark. Indexed,
bounded complete-selection reads use only this downstream database, reject
substituted receipts and verify ordered postings against decoded witnesses.
Direct adapter rebuild retries still re-evaluate. The opt-in Penelope coordinator
now skips that evaluation for validated completed requests and verifies retained
claims against the receipt instead.

This first schema does not yet persist invalidation intervals/reasons, perform
incremental synchronization/new-claim discovery. It must not be advertised as the complete effect contract
above. Canonical stores, core and public/runtime DTO crates gain no SQLite
dependency. The adapter is a separate workspace crate, not an engine default.

Storage verification: three adapter tests pass, covering explicit migration and
checksum/unrelated-database rejection; injected transaction failure with zero
partial operations, claims, postings or watermark; pinned rebuild/query
equivalence, idempotent retry, exact read-byte boundaries, reopen, substituted
receipts, posting tampering, non-regressing historical rebuild and same-identity
restoration producing a fresh derivation. Claim payloads include a BLAKE3 digest
checked before bounded decoding. Strict adapter Clippy and the architecture gate
pass. The existing four Query lifecycle fixtures, SQLite/Turso inference-history
fixtures, eighteen pure inference tests and Rust-source integration also pass.
The preceding full workspace CI completed in 401.78 seconds; it does not certify
the subsequently added storage crate, which has targeted verification above.

Penelope verification: all twenty-three integration tests pass with
`inference-sqlite`, including File and Turso journal restart recovery after an
unavailable effect and after SQL commit before journal completion. They verify
exact retained claims, idempotent completion, unchanged canonical history,
corrupt-journal rejection and failure when completed downstream data is missing.
The identity fixture checks destination binding, immutable execution controls,
receipt/saga-state agreement and rejection of trailing bytes. Journal rule
admission is capped at 8 MiB before rule cloning; records are capped at 16 MiB
before encoding and during decoding. Strict feature-enabled all-target Clippy
and the architecture gate pass. This does not validate automatic orchestration,
incremental lifecycle or user-facing inference commands.

The pure rule-set identity helper is implemented first: canonical ordered rules
encode complete semantic configuration under a versioned domain separator.
Rule-count and encoded-byte admission bound identity construction; budgets are
not hashed. Duplicate identity/version pairs and empty rule identifiers reject.
Scope/generation remain separate operation-identity components. This helper does
not establish ontology compatibility or replace evaluator validation.

Verification: seventeen inference unit tests and Rust-source integration pass.
Rule-set identity is invariant under reordered rules and enlarged admission
budgets, and changes with rule version, ontology version, configuration fingerprint
or output relation. Duplicate and oversized rule sets reject. Strict all-target
inference Clippy passes. No durable storage or Penelope coordinator is implemented
by this identity helper.

The pure `materialization_identity` helper now binds that rule-set identity to
repository, worktree and retained generation under a separate versioned domain.
Admission budgets are not semantic identity inputs; changing any scope coordinate
or rule version changes the operation identity. Storage destinations remain a
coordinator request concern and must still bind the journal request digest: equal
semantic operations do not authorize substituting an effect destination.
Eighteen inference unit tests, the Rust-source integration and strict all-target
Clippy pass. This provides identity construction, not durable effect execution.
