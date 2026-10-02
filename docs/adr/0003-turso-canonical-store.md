# ADR-0003: Turso WAL store schema and transaction boundary

- Status: accepted
- Date: 2026-09-27

## Context

The store port's reference implementations establish graph semantics, but the
product plan names embedded Turso WAL as the canonical local persistence
backend. The first adapter must preserve the pure synchronous graph-store
contract while using Turso's published async local API.

## Decision

Add `syntaxmesh-store-turso` as a host adapter. It requires WAL mode at open and
uses one current-thread executor internally to bridge the synchronous
`GraphStore` port. The domain, indexer, graph, query, and engine APIs remain
runtime independent.

Persist schema version, generation manifest, file versions, provenance, nodes,
and edges in separate tables. Encode each typed row with the current snapshot
codec and retain stable IDs as primary keys. Apply a validated candidate
generation with one Turso `IMMEDIATE` transactional batch that replaces all
rows and the manifest atomically. On open, reconstruct the reference semantic
store from those rows and compare the recomputed graph root with the stored
manifest before serving reads.

Before replacing rows, the adapter compares the full manifest observed when
the handle was opened with the persisted manifest inside the same immediate
transaction. Delta publication and verification-status updates therefore fail
as stale when another handle has advanced the generation or changed its
status; this compare-and-swap check is part of the write transaction.

## Consequences

- A failed delta validation or database transaction leaves the last published
  in-memory and durable generation unchanged.
- A restart validates that persisted graph rows reproduce the accepted root.
- The schema version is explicit; unknown versions fail closed. Future schema
  versions need explicit migration steps before opening.
- The in-memory semantic store remains the current write-staging and startup
  validation projection. Indexed Turso SQL reads are specified by ADR-0009;
  SQLite differential conformance remains subsequent work.
- The current-thread runtime bridge is scoped to this adapter; hosts using an
  async runtime call the synchronous engine at an application boundary and do
  not expose Turso runtime types through SyntaxMesh contracts.
