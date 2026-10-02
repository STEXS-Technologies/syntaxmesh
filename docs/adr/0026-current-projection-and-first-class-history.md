# ADR-0026: Separate the current graph projection from first-class history

- Status: accepted
- Date: 2026-09-27

## Context

ADR-0024 makes Turso's typed rows authoritative for the current graph and
applies row deltas transactionally. That is the right shape for current-state
queries, but replacing rows alone loses the prior versions. The SyntaxMesh v5
source of truth requires historical graph state to be first-class, important
facts to be versioned, and valid/system time to remain distinguishable from
observation/knowledge time (§§20, 22, 235–236, 327). A manifest hash chain is
not a historical fact graph, and Penelope's workflow log is not a history
query model.

## Decision

Treat the current graph as a materialized projection and committed generation
history as a separate, append-only part of canonical persistence. Every
accepted transition must preserve its parent/generation identity, complete
accepted delta (including upserts, removals, file changes, and provenance), and
the resulting manifest atomically with updating the current projection. A
baseline generation imported before history support may be retained as an
explicit anchor with no known predecessor delta; migration must never invent
older facts.

Accepted transitions are the auditable/rebuild source for historical state.
Temporal fact indexes and checkpoints—not replay of the full delta chain on
each request—serve normal historical queries. See
[ADR-0027](0027-layered-temporal-graph-queries.md). The temporal domain
contract must distinguish fact valid time from observation/acceptance time; a
generation sequence alone must not be misrepresented as wall-clock time.
StateChronicle may verify accepted history, but it does not replace graph facts
or their queryable history. Penelope coordinates durable indexing workflows,
but workflow records do not replace the history model. Current point and
neighborhood queries continue to use the current projection.

## Consequences

- The current-row optimization remains valid, but is not sufficient for the
  historical-graph acceptance gate. Per-request full-chain replay is only a
  bounded reference/rebuild path, not the production temporal query design.
- Store ports/backends need a durable generation-history contract, with
  atomic-commit, restart, replay, and historical-state conformance tests.
- Turso and SQLite schema upgrades need explicit history initialization; legacy
  data is a baseline anchor because prior deltas cannot be recovered.
- Public temporal/query contracts and any change to their encoding require a
  separate versioned fixture and migration decision before release.
- Historical storage may grow without bound initially; retention/compaction
  cannot discard the only reconstructible history and needs an explicit policy.
