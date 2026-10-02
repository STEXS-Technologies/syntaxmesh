# ADR 0010: SQLite Reference and Conformance Store

- Status: accepted
- Date: 2026-09-27

## Context

The SyntaxMesh product architecture names Turso as the canonical embedded live store and requires SQLite compatibility as a fallback/reference backend behind the same `GraphStore` port. The current conformance fixture compares InMemory, File, and Turso behavior, leaving SQLite-specific transaction and restart semantics untested.

## Decision

Add `syntaxmesh-store-sqlite` as a distinct adapter implementing `GraphStore` and `DurableRecordStore` with rusqlite from crates.io. It is a reference/fallback backend, not a dependency of core, query, or engine and not a replacement for Turso as the default. Graph rows, manifest, and operation records are persisted transactionally; the same InMemory semantic implementation is used to validate deltas and recompute graph roots. The backend must be exercised by the shared differential conformance scenario, including restart and stale-writer cases.

Use rusqlite's bundled SQLite build so consumers of the reference backend do not inherit a system SQLite installation requirement. Schema versions fail closed unless an explicit migration is implemented.

## Consequences

- Backend behavior can be differentially checked without changing application contracts.
- SQLite storage remains intentionally simple and may share the reference semantic implementation; it is not evidence of Turso performance or production behavior.
- The extra bundled native dependency increases build time and binary footprint for consumers that opt into this crate.

## Verification

- Add SQLite to the same update/delete/rollback/restart and indexed-read differential fixture as InMemory, File, and Turso.
- Exercise stale concurrent handles and durable-operation-record compare/exchange after reopen.
- Run strict workspace CI and the architecture dependency checker.
