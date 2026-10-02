# ADR-0049: Compact completed Penelope indexing records

- Status: accepted
- Date: 2026-09-28

## Context

The repository benchmark showed that two SQLite/Turso Penelope records held
65,514,741 bytes of payload for a two-generation, 696-file Rust workload. The
large row is the completed initial indexing request. Prepared records need
their full graph/lineage/consequence request to recover an interrupted
publication. Completed records are terminal: recovery ignores them, while
idempotent retry only needs to prove the request is identical and return the
accepted generation. Keeping the full request in a terminal record duplicates
the canonical delta history and current/history projections indefinitely.

Penelope remains mandatory for the full-engine publication workflow. Its
append-only event sequence and the accepted generation manifest remain durable;
this decision does not trim workflow events or weaken prepared-run recovery.

## Decision

1. Keep full request payloads in prepared records. Recovery and reconciliation
continue to use the exact existing v1-v4 record forms.
2. After Penelope has replayed the successful terminal event sequence and the
   graph generation has committed, atomically replace the prepared record with a
   version-6 completed record containing the run ID, accepted-time metadata,
   Penelope event sequence, accepted manifest, request digest, and process
   definition digest. Do not retain the full graph, ChangeSet, or consequence
   deltas there. For new operations both digests are equal.
3. Reconstruct the deterministic Penelope process definition from its stored
   definition digest and run ID when replaying a compact terminal record. Compare
   an exact retry against the separately stored digest of the complete incoming
   request; changed requests are rejected as run-ID reuse. Preserve the existing
   empty-delta retry exception: when no graph/lineage/consequence mutations were
   requested and the accepted generation is still current, a retry with an
   advanced expected-base cursor returns that same generation.
4. Continue decoding v1-v5 records, including full-payload and compact records.
   Existing rows are not rewritten during open or migration. An exact retry
   compare-and-swaps any completed v1-v4 record into compact v6. The separate
   request and definition digests preserve replay for legacy v1-v3 processes,
   whose definition payload differs from the current complete-request digest.
5. Keep generation history as the canonical graph-delta history. The compact
Penelope record is a workflow receipt/event record, not a second history store.

## Consequences

- New completed operations have storage proportional to workflow events and
  manifest metadata, rather than graph-delta size.
- Prepared crash recovery, CAS transitions, and Penelope replay validation stay
  unchanged.
- Older completed rows remain readable and may retain their prior storage
  footprint until exact retry or a future explicit maintenance operation; open
  does not rewrite them.
- Repeated completed requests remain idempotent without re-reading or replaying
  graph deltas; a payload digest mismatch remains a hard conflict.
