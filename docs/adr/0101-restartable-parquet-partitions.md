# ADR-0101: Restartable fact-history Parquet partitions

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0098 requires deterministic Parquet artifacts and restartable progress that
can be rebuilt from canonical history. ADR-0099 provides bounded,
snapshot-pinned fact pages, and ADR-0100 maps each page to a versioned Arrow
batch. Shardline already uses `parquet::arrow::ArrowWriter` with explicit row
group properties and publishes local files through a temporary-file rename.

The Arrow feed should remain free of filesystem and Parquet dependencies.
Publishing one large Parquet file makes a long export expensive to resume;
writing NDJSON checkpoints would add a text serialization path contrary to the
analytics boundary.

## Decision

1. Add a separate `syntaxmesh-analytics-parquet` Rust adapter. It consumes the
   typed Arrow page reader and writes one deterministic Parquet file per page.
2. Store a compact versioned `manifest.json` beside sequentially named
   partitions. It binds the dataset to repository, worktree, generation, Arrow
   schema version, committed partition/row counts, completion state, and the
   last committed history cursor. Each partition has a small BLAKE3 checksum
   sidecar. The manifest is ordinary JSON metadata, not NDJSON and not the
   canonical history store; its size stays constant as history grows.
3. Write each partition to a temporary file in the destination directory,
   close and sync it, then atomically publish it before advancing the manifest.
   If a process stops between those steps, the unlisted partition is verified
   against the deterministic retry and then adopted. A manifest only advances
   after its partition and checksum are durable.
4. Re-running against an incomplete manifest resumes from its cursor. A
   completed matching export is idempotent; a different repository, worktree,
   generation, schema, corrupt latest checkpoint partition, or incompatible
   manifest fails closed. The caller chooses a new destination for a new
   snapshot. The separate explicit full-dataset audit is defined by
   [ADR-0102](0102-full-parquet-export-integrity-audit.md).
5. This operation is explicit and downstream. It never runs in graph
   publication and cannot block or roll back canonical writes. DuckDB remains
   optional and outside this adapter.

## Alternatives considered

- One monolithic Parquet file: rejected for the initial exporter because it
  cannot checkpoint page-level progress without rewriting the file.
- NDJSON or a JSON-lines progress journal: rejected; the durable analytical
  data is typed Parquet, with one small versioned JSON manifest.
- Adding Parquet/filesystem dependencies to `syntaxmesh-analytics`: rejected;
  the typed Arrow feed remains independently usable in memory.
- Synchronous OLTP/DuckDB dual writes: rejected by ADR-0098.

## Verification

- Exported partitions are readable Parquet with the Arrow schema metadata and
  deterministic page ordering.
- The constant-size manifest advances only after a partition and checksum are
  durably published; a retry adopts a matching orphan partition and does not
  duplicate committed pages.
- Completed exports are idempotent; identity mismatch and latest-checkpoint
  hash corruption fail closed, while canonical graph state remains unchanged.
- The adapter depends on the analytics/store ports, not on concrete SQLite or
  Turso layouts, and adds no NDJSON path.
