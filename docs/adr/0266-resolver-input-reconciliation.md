# ADR-0266: Resolver-input reconciliation

Status: accepted, 2026-10-01.

## Decision

The shared structural reconciler includes persisted and currently observed
resolver inputs in its existing source-planning fingerprint. Reuse the scoped
durable records, host content fingerprinting, Indexer preparation and Penelope
publication rather than introducing a second dependency database or workflow.

When preparation discovers new input paths, discard the candidate and repeat
with the expanded inventory fingerprinted before preparation. Allow at most
three passes; unstable coverage fails closed. Compare known inputs before and
after preparation and reject content changes before publication. Persist coverage
before acceptance so restart cannot acknowledge a generation without coverage.
Planning markers remain intents, not accepted generations.
Persisted coverage accumulates paths within a resolver policy, including across
provider replacement; narrowing it would invalidate the just-accepted planning
fingerprint and cause a spurious subsequent publication. Coverage pruning is a
separate future policy, not an implicit side effect of resolver refresh.

## Limits

This detects changes between samples, not adversarial ABA mutations or an atomic
filesystem snapshot. Providers must report consulted paths. Sources are already
captured as immutable scanned bytes. The CLI still has a separate indexing path;
this decision initially applies to shared structural reconciliation and daemon
hosts. Host IO failures use the existing Store error channel, with diagnostics.
