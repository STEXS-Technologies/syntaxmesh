# ADR-0180: Shared context host adapters

Status: Accepted

## Decision

Move the existing MCP repository source reader and exact tiktoken counter into
`syntaxmesh-context-host`, a concrete host-adapter crate. Reuse the existing
normalized-path and canonical-root checks and bounded token memo rather than
implementing another copy for HTTP context retrieval. The query compiler remains
responsible for source-content hash verification and context selection.

The shared constructors return diagnostic strings; transport hosts determine
whether and how these are exposed. Supported encodings and tokenizer identities
remain unchanged. `for_request` shares the initialized encoding but creates a
fresh bounded memo so source text is not retained across requests. MCP keeps
its test-only profiling wrapper.

Filesystem and tokenizer dependencies stay in this adapter, not core, public
DTOs, or thin runtime protocol. No storage, workflow, transport, or async runtime
is added to it. Canonical-path checks are not a race-proof filesystem sandbox;
repository roots must remain operator-controlled.

## Scope and verification

This is shared infrastructure for the planned HTTP context route, not evidence
that that route exists. Preserve MCP source escape and token-budget tests and
move exact memo equivalence tests with the implementation. Verify both hosts
still compile and run strict Clippy before adding the HTTP public contract.

Verification: `cargo make ci` completed successfully on 2026-09-30 in 127.62
seconds, including architecture boundaries, independent extension import,
migration registries, strict workspace Clippy, dependency policy, workspace
tests/backend conformance, and documentation generation. Shared adapter tests
cover both encodings and request memo isolation; existing MCP tests cover source
escape, serialized token budgets, historical source verification, and stdio.
