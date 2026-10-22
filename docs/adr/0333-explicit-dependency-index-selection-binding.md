# ADR-0333: Explicit dependency-index selection binding

Status: implemented.

Truth-maintenance consumers must bind dependency lookup to the claim selection,
not the generation producing a later change feed. Introduce a pure typed
`ClaimSelection` (repository, worktree, generation) and checked index construction
and lookup. Explicit construction supports empty complete claim sets without
losing their selection. Reject every mixed claim and foreign lookup selection
before matching postings. Existing low-level construction/lookup remains for
trusted in-process callers; durable workflow integration must use checked APIs.

This does not infer generation ancestry, authenticate external claims, or make a
change-feed generation interchangeable with the retained claim selection.

Verification: eight inference unit tests plus Rust extraction integration pass.
Scope tests reject foreign repository, worktree and generation labels for both
populated and empty selections. Shared four-backend change-feed lifecycle tests
use the checked APIs and pass; strict inference/Query/SQL Clippy passes.
