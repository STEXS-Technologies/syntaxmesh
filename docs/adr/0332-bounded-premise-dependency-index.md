# ADR-0332: Bounded premise dependency lookup

Status: implemented for direct, in-memory dependency lookup.

Source-of-truth sections 80 and 316 require premise-to-derivation lookup for
truth maintenance. Add a pure optional `PremiseDependencyIndex` over complete
claims from one repository/worktree/generation. Index exact `FactVersionRef`
keys, not fact identities alone: a different retained version must not silently
invalidate an old witness. Reject duplicate claim IDs, empty premises and mixed
selection labels. Build limits bound claims and premise visits.

Lookup accepts changed versions with explicit work/output limits. Return every
affected claim ID or an error, never a silently truncated invalidation set.
Reuse ordered maps/sets and existing exact witness metadata; no graph replay,
rule execution, durable write, transport or host dependency is introduced.
Lookup is logarithmic plus posting/output work, not an O(1) claim.

This identifies directly affected claims. It does not retract them, traverse
derived-claim DAGs or implement Penelope-managed durable truth maintenance.
Verify against a witness-scanning oracle, including unchanged versions, duplicate
posting hits, mixed selections and budget exhaustion. Durable propagation remains
an Engine workflow requirement, outside this pure index.

Verification: the inference crate's seven unit tests and Rust-source integration
test pass, including exact-version lookup compared with a witness-scan oracle,
deduplicated affected claims, mixed-scope rejection and fail-closed budgets.
Strict Clippy passes for all inference targets. This does not constitute
verification of durable propagation, which is not implemented here.

The shared Query lifecycle fixture also consumes the existing generation-pinned
fact-version change feed one entry per page, selects closed exact versions and
verifies their postings identify the retained dependent claim. This passes on
InMemory, File, SQLite and Turso, including reopened durable stores. This is
cross-backend integration evidence for the invalidation input, not an automatic
invalidation workflow or proof of delta-proportional reference-store internals.

The same four-backend fixture restores the deleted declaration using its original
edge identity. The new claim witnesses the restoration generation; the old index
does not match that new version, the new index does, and the retained historical
claim remains unchanged. Targeted Query/SQL lifecycle tests and strict Clippy pass.
