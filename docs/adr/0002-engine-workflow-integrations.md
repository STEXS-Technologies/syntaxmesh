# ADR-0002: Engine composition and workflow integrations

- Status: accepted
- Date: 2026-09-27

## Context

The first vertical slice had a runtime-neutral indexer and reference stores, but
the CLI could bypass the application boundary. The architecture requires an
engine that composes indexing, persistence, queries, Penelope publication, and
optional StateChronicle verification without leaking those dependencies into
the pure crates.

## Decision

Add `syntaxmesh-engine` as the application composition root. It prepares a
`GraphDelta`, publishes it through a borrowing Penelope adapter, and verifies
the accepted manifest through a StateChronicle digest adapter before returning
the receipt. The CLI uses this engine for indexing; query reads remain
generation-scoped through the query port.

The Penelope adapter uses Penelope's deterministic saga planner for the
publication step and the SyntaxMesh store port for the atomic graph commit.
The StateChronicle adapter records a canonical digest of the accepted manifest.
This is the first integration contract; durable saga/history backends and the
canonical Turso store remain later implementation gates.

## Consequences

- Pure domain, DTO, protocol, and extractor crates remain free of workflow and
  database dependencies.
- Embedded and CLI hosts share the same engine publication semantics.
- A later durable Penelope/StateChronicle backend can replace the adapters
  without changing indexer or query contracts.
- The reference file store remains explicitly non-production until the Turso
  WAL adapter passes backend conformance tests.
