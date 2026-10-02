# ADR 0017: Return actionable extension-validation diagnostics

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

The engine rejects malformed extension batches before creating Penelope records
or changing canonical graph state. The rejection is returned as a typed
`EngineError`, but its `Display` implementation previously emitted only Rust
debug variant names, which are not useful to an extension author or host
operator.

## Decision

Keep validation failures synchronous and point-of-use: return the existing
typed error and render it as an actionable human-readable message. Do not
persist rejected payloads in Penelope or canonical graph state. A host may
apply its own logging or retention policy without the engine imposing one.

## Consequences

- Extension authors can identify common invalid-manifest, capability, namespace,
  ownership, and observation failures from the error returned by the public
  engine API.
- A rejected batch still creates no workflow record and cannot affect graph
  state or another producer's publication.
- This is not a durable extension-failure ledger or aggregated status metric;
  hosts that require historical diagnostics must own that policy. Panics and
  timeouts remain outside the in-process extension API's isolation boundary.

## Alternatives considered

- Store rejected payloads in Penelope: rejected because validation happens
  before workflow admission and rejected input should not become workflow state.
- Add a canonical diagnostic graph fact: rejected because diagnostics are not
  source knowledge and must not alter accepted generations.
- Keep debug-formatted errors: rejected because callers cannot reliably
  communicate corrective action to users.

## Migration and compatibility

No error variants, persisted records, public DTOs, canonical facts, or schemas
change. The human-readable text returned by `Display` becomes descriptive;
callers should match typed variants rather than depend on message strings.
