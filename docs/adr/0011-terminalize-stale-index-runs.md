# ADR 0011: Terminalize Stale Prepared Index Runs

- Status: accepted
- Date: 2026-09-27

## Context

Indexing prepares a `GraphDelta` against an expected generation, then publishes it through Penelope. A newer run can publish first. The store correctly rejects the delayed delta as stale, but the durable Penelope record currently remains `Prepared`; startup recovery retries it and can prevent later indexing indefinitely.

## Decision

When publication/recovery proves that an index delta's expected base is stale and the target generation was not already accepted as that exact delta, append a Penelope terminal-failure event and compare-and-swap the operation record to a terminal `Rejected` phase. Return the original stale-base error to the caller. Recovery skips terminal rejected records and continues processing other prepared work. Transient backend failures and ambiguous outcomes remain `Prepared` for retry/reconciliation.

Append the new record phase without changing the persisted record schema or existing enum variant order. Existing prepared records remain recoverable.

## Consequences

- A late parser/index result cannot overwrite a newer generation and cannot poison subsequent recovery.
- Rejection is durable and replay-verifiable as a terminal Penelope outcome.
- Callers must start a fresh index run from the current generation after stale rejection.

## Verification

- Prepare two real index deltas from the same base, publish the newer delta first, then assert the delayed run is rejected and recorded terminally.
- Recover pending records and publish another valid generation successfully.
- Repeat across workspace CI and existing Penelope restart/recovery tests.
