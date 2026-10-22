# ADR 0378: MCP generation-pinned contract evaluation

Status: accepted

Date: 2026-10-22

## Context

The pure evaluator and Query API can evaluate explicit contracts against a
retained generation, but agent harnesses cannot use this capability through the
existing read-only MCP host. Duplicating the operation in a harness would weaken
generation pinning and evidence semantics.

## Decision

Add a versioned, read-only `evaluate_contracts` MCP tool. The caller supplies at
most 32 versioned contracts, stable source/target node IDs, an explicit
core/external relation descriptor, required/forbidden expectation, and an
optional retained generation. The host binds all contracts to `Declared`
origin, applies hard upper bounds to contract count, request bytes, retained
history scan, and edge visits, and delegates through Query to the pure
evaluator. The response includes the pinned generation, deterministic result
identity, outcome, and exact evidence version when present. Missing relation
evidence remains `InsufficientEvidence`. The tool performs no writes and
exposes no database paths or runtime-specific inference configuration.

## Consequences

- Any MCP client can request the same evidence-backed contract evaluation
  without a Codex-specific integration or duplicated inference logic.
- The MCP tool is an adapter over the public Query behavior; policy severity,
  persistent contract authoring/results, and host mutation workflows remain
  outside this operation.
- The request/result schema is versioned and bounded. The server selects a
  generation if omitted and validates repository scope when one is supplied.

## Verification

Test tool discovery, current and retained generation selection, supported,
contradicted, and insufficient-evidence responses, invalid relation/identity
inputs, and rejected bounds. Verify a real MCP request/response over stdio,
strict Clippy, architecture boundaries, and full workspace CI.

## References

- ADR-0376, evidence-bounded semantic contract evaluation.
- ADR-0377, generation-pinned contract query.
- SyntaxMesh source of truth v5, sections 130–133, 221, and 223.
- `crates/syntaxmesh-mcp/src/server.rs`.
