# ADR 0062: Local Git context at the CLI boundary

Status: accepted

Date: 2026-09-28

## Context

The v0.1 scope calls for Git HEAD/working-tree awareness so users can distinguish the checkout whose graph they are querying. SyntaxMesh currently scopes each index to the canonical source-root path, which keeps separate worktrees isolated, but does not expose branch, HEAD, or dirty state. Shardline's `shardline-vcs` crate is for remote hosting-provider adapters and is not a local-worktree implementation to reuse.

## Decision

Add a CLI-only `git-context <source-root>` diagnostic. It asks the installed Git executable for the top-level worktree, symbolic branch (or detached state), HEAD commit, and porcelain dirty state. Git discovery is best-effort outside repositories, reports Git-unavailable/errors explicitly, and does not affect indexing, generation identity, repository/worktree IDs, persisted schemas, or engine APIs. Local Git integration remains outside core, stores, and runtime protocols.

## Consequences

Users can identify the checkout and source state associated with a query without adding Git dependencies or changing durable graph contracts. The result is a point-in-time diagnostic, not a historical claim; commit identity and working-tree contents are not yet recorded per generation.

## Verification

CLI integration tests cover a normal branch, detached HEAD, dirty state, and a non-repository directory. The command remains functional when Git is unavailable.

## References

- [SyntaxMesh §56](../syntaxmesh-todo-extensible-opensource-v5.md)
- [v0 architecture plan, Gate 5](../V0_ARCHITECTURE_PLAN.md)
