# ADR 0018: Report logical graph integrity in engine status

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

The store adapters validate graph roots during Turso and SQLite restore, but
the restartable File store only decodes its snapshot and rebuilds indexes.
Engine and CLI status expose generation and counts without a common integrity
result. Operators therefore cannot distinguish a successfully opened but
logically inconsistent reference snapshot from a consistent generation.

## Decision

Have the runtime-neutral engine status path recompute and report two logical
invariants over the current generation:

1. The root recomputed from the ordered canonical nodes and edges matches the
   manifest's graph root.
2. Every node/edge provenance reference exists and every edge endpoint exists.

Return these as explicit booleans on the engine status DTO and print
`logical_integrity=ok` only when both pass; otherwise print the individual
failed checks and return a non-zero CLI status. Status remains read-only.
Store open/restore errors continue to fail closed before status is available.

## Consequences

- Embedded hosts and CLI consumers see the same runtime-neutral logical check.
- The check validates canonical graph consistency, not physical database pages,
  filesystem durability, index lag, or backup recoverability.
- The status operation reads the full current generation, which matches the
  current status implementation's fact-count reads; this is not a large-graph
  integrity scan optimization.
- Adding fields to `EngineStatus` is an additive Rust API shape change; callers
  constructing it directly must provide the integrity result.

## Alternatives considered

- Report `ok` merely because the backend opened: rejected because FileGraphStore
  does not currently recompute the graph root on open.
- Make status repair or rebuild the store: rejected because diagnostics must be
  read-only and recovery is a separate operator action.
- Claim physical-database integrity from the graph-root check: rejected because
  logical graph consistency is narrower than a backend page/WAL audit.

## Migration and compatibility

No stored schema, public graph DTO, root algorithm, or canonical fact changes.
The engine status Rust struct gains an integrity field; CLI status gains
explicit logical-integrity output.
