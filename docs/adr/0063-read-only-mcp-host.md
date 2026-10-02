# ADR 0063: Read-only MCP host over the query API

Status: accepted

Date: 2026-09-28

## Context

The v0.1 plan lists MCP/context access as unfinished. SyntaxMesh already has generation-pinned read services and a standalone CLI, while sibling `bug-bounty-harness` demonstrates a useful MCP composition: an official `rmcp` tool server behind a small stdio binary. Its domain-specific authorization and subprocess behavior are not reusable here. MCP and Tokio must not enter core, DTO, query, store, or engine crates.

## Decision

Add a separate `syntaxmesh-mcp` host crate using the official `rmcp` SDK and stdio transport. It opens an already-migrated Turso database, composes the shared `SyntaxMeshEngine`, pins each tool call to the latest published generation when the host starts, and delegates reads through `SyntaxMeshEngine::query`/`status`. Because the current store adapter rejects reads from a superseded open handle, the host checks its generation before and after each call and returns a restart-required error instead of silently switching graphs. The initial public tools are `status`, `search`, `node`, `neighbors`, and bounded directed `path`. No indexing, migration, mutation, runtime observation, or context-budget claim is exposed. The host owns its Tokio runtime; engine/query contracts remain synchronous and runtime-neutral. Synchronous Turso operations execute on blocking workers so its adapter-owned runtime is not nested on the protocol runtime.

## Consequences

Agent clients get a protocol-native read surface without coupling embedded consumers to MCP. An MCP process must be restarted to observe a newly published generation; this avoids mixing generations during a session. Context compilation, token budgets, refresh notifications, and writes remain future work.

## Verification

Tests cover tool discovery and bounded query results against an indexed fixture, including a generation-pinned session and invalid IDs/limits. Workspace dependency checks verify `rmcp` and Tokio are confined to the MCP host.

## References

- [SyntaxMesh §74](../syntaxmesh-todo-extensible-opensource-v5.md)
- [SyntaxMesh §133.2](../syntaxmesh-todo-extensible-opensource-v5.md)
- [v0 architecture plan](../V0_ARCHITECTURE_PLAN.md)
- Shardline migration patterns remain recorded at [ADR-0046](0046-sqlite-migration-operations.md); MCP server composition is adapted from the sibling bug-bounty harness.
