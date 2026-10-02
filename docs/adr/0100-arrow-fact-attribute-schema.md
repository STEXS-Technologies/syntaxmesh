# ADR-0100: Versioned Arrow fact-attribute schema

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0098 calls for typed Rust/Arrow analytics handoff and Parquet as an explicit
artifact; ADR-0099 supplies bounded, snapshot-pinned fact-history pages. The
canonical fact payloads are intentionally extensible: new language and plugin
facts can add variant-specific fields without changing the graph-store
contract. A fixed wide Arrow row would force every extension field into the
core analytics schema, while opaque serialized payloads would not be useful to
DuckDB queries.

Shardline already uses Arrow `RecordBatch` values and Parquet row groups in
Rust. Reuse those direct crates.io libraries and their tested batch-building
approach, but define SyntaxMesh's own domain schema.

## Decision

1. The first Arrow feed is a normalized `fact_history_attributes` table: one
   row per typed attribute of one fact version. Every row repeats schema
   version, pinned generation, fact family/id, validity interval, and optional
   observation/acceptance time.
2. Attribute values are discriminated and typed as UTF-8, binary, unsigned
   integer, or boolean. Repeated values (for example, resolver candidate
   paths) carry a deterministic zero-based ordinal. Identifiers and content
   hashes remain 32-byte binary values, never formatted strings.
3. Each `FactPayload` family is expanded into stable dotted attribute names.
   Enum variants receive explicit stable names; all variant fields, extension
   payload bytes, source spans, and exact identities are preserved. Adding an
   attribute does not require a breaking Arrow schema change; changing common
   columns or value codes requires a schema-version increment.
4. `syntaxmesh-analytics` depends on the store port and Arrow crates only. It
   exposes a lazy page reader that returns one `RecordBatch` per bounded store
   page and exposes the last cursor for caller checkpointing. It has no
   filesystem, service, CLI, NDJSON, DuckDB, or SQL-adapter dependency.
5. Parquet partitioning/writing and DuckDB query integration remain separate
   downstream adapters. This crate provides Arrow batches; it does not make
   analytics part of graph publication or imply restartable file commits.

## Alternatives considered

- One wide row with nullable columns for every node/import/export/extension
  variant: rejected because each new extensible fact shape changes a broad
  schema and complicates cross-version readers.
- Opaque Bincode payload columns: rejected because they are not queryable by
  DuckDB and couple artifacts to an internal Rust encoding.
- JSON/NDJSON payload columns: rejected as text serialization overhead and
  unsuitable as the typed internal feed.
- A map encoded as parallel Arrow lists: rejected in favor of ordinary rows,
  which DuckDB can filter, aggregate, join, and partition directly.

## Verification

- The Arrow fixture covers file/provenance/node/edge payloads, optional fields,
  extension bytes, exact IDs/spans, and enum variant details; broader
  cross-backend Arrow equivalence remains open.
- The fixture checks dataset/schema metadata, binary ID width, repeated-value
  ordinals, bounded pages, and cursor resumption.
- The workspace architecture gate confirms Arrow crates remain outside core,
  API DTO, runtime protocol, and store crates.
