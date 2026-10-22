# ADR-0373: Resolve exact fact-version retirement events

- Status: Accepted
- Date: 2026-10-22

## Context

ADR-0372 resolves the accepted transition that introduced an exact retained
fact version. v5 Stage B also calls for `ResolvedBy`, but a version's closing
transition is not necessarily semantic resolution: it may be an update or a
removal, and it does not establish cross-fact causation.

## Decision

Add a generation-pinned read that returns the exact retained fact version and
the accepted event at its exclusive `valid_until` boundary, only when that
event directly records the fact as `Updated` or `Removed`. A version still
active at the selected generation, an unknown version, and a closure later
than the selected generation return no result. The record is named
`FactVersionRetirement`, not `ResolvedBy`, to avoid claiming domain-level
resolution. No storage migration is needed: retained fact history and direct
change-event lookup are already indexed by durable stores.

## Consequences

- The query is bounded to one fact's retained versions and one exact closing
  event; SQLite/Turso use existing history and event indexes.
- The result identifies direct transition membership only. It does not infer
  why the fact changed, whether the underlying issue was fixed, or whether
  another fact caused the transition.
- Temporal export schema increments because a public record variant is added.
