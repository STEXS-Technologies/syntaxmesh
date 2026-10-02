# ADR 0016: Expose read-only workflow recovery diagnostics

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

The engine automatically recovers prepared Penelope operations before the next
index or extension publication, and hosts can request recovery explicitly.
However, `status` only reports the current graph generation and fact counts.
Operators cannot tell whether a durable operation is prepared, completed, or
rejected without reaching into backend-specific record keys or triggering
recovery as a side effect.

## Decision

Expose read-only Penelope operation counts through the workflow adapter and
runtime-neutral engine diagnostics API. Report prepared, completed, and
rejected operation records. Decode/validation failures are returned as
diagnostic errors instead of being hidden. CLI `status` displays these counts
without mutating the store; explicit recovery continues to use the existing
engine recovery method.

## Consequences

- Hosts can distinguish pending recovery from a completed/rejected workflow
  without parsing Penelope's private serialization or record-key format.
- Diagnostics do not perform recovery and do not change the graph generation.
- Counts describe durable index-operation records only; they do not yet report
  extension failures, scan/index lag, or a full backend integrity audit.
- Historical records remain durable and are counted cumulatively; compaction
  or retention policy remains open.

## Alternatives considered

- Recover automatically during `status`: rejected because a read-only
  diagnostic should not mutate the system it is reporting.
- Expose raw operation records: rejected because it leaks Penelope's internal
  state machine and serialization into host/public APIs.
- Leave recovery implicit until the next index: rejected because operators
  cannot identify a prepared run or distinguish it from an idle store.

## Migration and compatibility

This adds read-only diagnostics types and methods without changing persisted
records, canonical graph facts, generation identity, or database schemas.
