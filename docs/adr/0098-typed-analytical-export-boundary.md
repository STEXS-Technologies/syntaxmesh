# ADR-0098: Typed, downstream analytical export boundary

- Status: accepted
- Date: 2026-09-29

## Context

The SyntaxMesh product source identifies DuckDB/Parquet as the analytical
layer for cross-generation scans, while Turso/SQLite remain the canonical
transactional stores. It explicitly rejects synchronous dual writes and asks
for a stable analytical interchange schema independent of SQL table layouts.
No analytics crate or synchronization path exists yet.

NDJSON is already a versioned human/tool-facing query and graph export format.
It is not an appropriate internal analytics transport: serializing canonical
facts to text and parsing them back would add avoidable CPU and allocation, and
would couple analytics to a user-facing export representation. Shardline's
Arrow/Parquet use demonstrates a Rust-native, columnar file boundary that can
be reused for durable analytical artifacts, but SyntaxMesh must retain its
runtime-neutral engine and canonical-store ownership rules.

## Decision

1. Add analytics as a downstream Rust library/adapter, separate from core,
   public graph DTOs, runtime protocol, query semantics, and OLTP stores.
2. Define a versioned, typed analytical schema and read-only feed boundary.
   The feed is populated from canonical store/query contracts, never by
   depending on SQLite/Turso private table layouts.
3. Use in-process typed Rust values/Arrow record batches for internal handoff.
   Use Parquet (and optionally Arrow IPC) as durable analytical export/import
   artifacts. NDJSON remains for explicit user/tool graph and query exports;
   it is not used as the analytics staging store or an internal service
   protocol.
4. Canonical generation publication does not synchronously write DuckDB or
   analytics files. Export/synchronization is an explicit, retryable downstream
   operation. Analytics failures or lag cannot roll back, block, or alter an
   accepted graph generation. Analytics state is rebuildable and must expose
   the generation/cursor through which it is complete.
5. The initial slice exports accepted generation and typed fact-history data
   with deterministic ordering, schema-version metadata, bounded batches, and
   restartable progress. Aggregates and DuckDB SQL interfaces follow after the
   feed round-trips across store backends.
6. Keep DuckDB integration optional and outside core/engine construction unless
   explicitly requested by a host. The analytics contract and Parquet export
   remain usable without a DuckDB dependency.

## Alternatives considered

- Write Turso/SQLite and DuckDB in one publication transaction: rejected;
  this creates dual-write failure modes and violates the source architecture.
- Treat NDJSON as an internal feed: rejected; it is a public export format and
  incurs text encode/decode overhead for a Rust-local boundary.
- Read canonical SQL tables directly from DuckDB: rejected; it couples
  analytics to adapter schemas and migrations.
- Make DuckDB a core/engine dependency: rejected; hosts that do not need
  analytics would inherit its build/runtime cost and storage concerns.
- Begin with broad analytics tables and metrics: deferred until the versioned
  fact feed and restartable export are verified.

## Consequences

- Public analytics schema changes require explicit versioning and migration
  tests, but do not change graph-store migrations.
- The initial work is a versioned typed feed plus deterministic Parquet export;
  it is not an analytics query engine or an exactly-once cross-store commit.
- The store/query API may need a bounded generation/fact-history scan port;
  decide that contract only when the first exporter fixture demonstrates the
  required ordering, continuation, and failure semantics.
- Shardline's Arrow/Parquet construction and artifact-verification practices
  are reusable; its domain schemas and service/storage dependencies are not.

## Verification

- The same analytical export is byte/logically equivalent across InMemory,
  File, SQLite, and Turso for a fixed fixture.
- A failed or interrupted export leaves the canonical store unchanged and can
  resume or safely rebuild from its generation cursor.
- Export ordering, schema version, generation identity, and content hash are
  deterministic; unsupported schema versions fail closed.
- No core/API-model/runtime-protocol or canonical publication dependency on
  DuckDB, Arrow, Parquet, or NDJSON serialization is introduced.
