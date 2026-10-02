# ADR-0005: Durable operation records for recoverable workflows

- Status: accepted
- Date: 2026-09-27

## Context

Penelope is a pure process planner. The current adapter plans an index
publication but has no durable process log, so a restart cannot distinguish an
interrupted run from a completed one or replay the pending publication. The
graph store must not take a dependency on Penelope or engine workflow types.

## Decision

Add an opaque compare-and-swap record capability alongside `GraphStore` in
`syntaxmesh-store`. Records use namespaced string keys and byte payloads, and
support exact-key reads, prefix scans for recovery, and conditional writes.
Memory, file-snapshot, and Turso stores implement this capability. Turso keeps
records in a separately migrated table; a Penelope integration serializes its
own versioned process record there without leaking Penelope types into storage.

An indexing process record will retain the prepared graph delta and Penelope
event sequence before publication. Recovery will replay the recorded Penelope
events and reconcile the delta against the current manifest: an already
published generation is completed idempotently, a still-valid base can be
published, and a conflicting base is surfaced rather than overwritten.

## Consequences

- Store crates remain workflow-agnostic and may be used by other durable
  operations through the same opaque record capability.
- Record compare-and-swap is independent from graph-generation compare-and-swap;
  recovery is responsible for reconciling the two durable facts after a crash.
- Completed process records are retained for idempotency and diagnostics; a
  later retention policy must not remove records needed for recovery.
- File snapshots are a durable reference backend; in-memory records are only
  suitable for tests and non-durable hosts.
