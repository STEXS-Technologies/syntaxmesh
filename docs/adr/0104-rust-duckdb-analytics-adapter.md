# ADR-0104: Rust DuckDB analytics adapter over verified Parquet

- Status: accepted
- Date: 2026-09-29

## Context

The product source requires DuckDB for downstream historical aggregation and
cross-snapshot analytics. The typed Arrow feed, versioned Parquet partitions,
full export verifier, and Penelope-managed export workflow already establish
the canonical-to-analytics boundary. DuckDB must not become a second live
graph store or a correctness dependency. SyntaxMesh also has an explicit
Rust-only execution boundary: supported source-language runtimes are never
launched by the engine.

Shardline's DuckDB design supplies two reusable principles: analytical work is
isolated from canonical publication, and server-facing query contracts are
structured, read-only, and bounded instead of arbitrary SQL. Its external
DuckDB/S3 workflow and Python real-client harness are not reused because
SyntaxMesh is a local Rust engine with no S3 frontend and no non-Rust execution.

## Decision

1. Add `syntaxmesh-analytics-duckdb` as a separate Rust adapter crate using the
   crates.io `duckdb` Rust binding. It is not a dependency of core, public DTO,
   query, runtime-protocol, indexer, engine, MCP, or thin runtime-probe crates.
2. The first query surface opens a completed fact-history Parquet export only
   after `verify_fact_history_export` has audited all partitions, checksums,
   schema, directory entries, and row totals. It binds the DuckDB view to the
   exact committed partition sequence and the export's repository, worktree,
   and `as_of_generation`; incomplete exports fail closed.
3. Expose typed Rust analytics operations with bounded result counts. Do not
   expose raw SQL through SyntaxMesh host/API contracts. DuckDB SQL remains an
   implementation detail of this adapter.
4. Use an in-memory DuckDB connection for this first read-only query slice.
   The authoritative Parquet artifacts remain the rebuildable analytics input;
   this adapter creates no competing durable database, internal protocol, or
   NDJSON intermediate. It starts no subprocess and has no daemon/background
   runtime.
5. Query and analytics failure cannot roll back, delay, or alter an accepted
   graph generation. Penelope-managed export remains the preceding durable
   effect; future materialized/incremental DuckDB sync requires a separate ADR
   and must remain replayable from canonical history/Parquet.

## Alternatives considered

- Embed DuckDB in core, query, or MCP: rejected because it pollutes
  runtime-neutral/hot-path boundaries and couples canonical reads to analytics.
- Spawn the DuckDB CLI or use Python: rejected by the Rust-only execution
  boundary and harder-to-bound process contract.
- Read adapter-private SQLite/Turso tables directly: rejected because it
  bypasses the stable versioned feed and backend interchange boundary.
- Expose arbitrary SQL: deferred; the source-backed first slice needs
  reproducible typed analytics, not an unbounded host-facing SQL surface.
- Persist a second DuckDB database immediately: deferred until incremental
  synchronization/recovery semantics and performance evidence are specified.

## Consequences

- Rust hosts can perform analytical queries over a verified immutable export
  without launching another runtime or affecting live graph correctness.
- The first query operations can be benchmarked against direct Arrow/Parquet
  scans before deciding on materialized DuckDB synchronization.
- DuckDB's bundled native engine increases build time and binary size; keep it
  confined to the explicit optional analytics adapter and measure those costs.
- Current query results are bounded, but scan work is proportional to the
  selected Parquet dataset. This is an analytics surface, not the O(1) live
  graph/history query path.

## Verification

- Reject incomplete, corrupt, wrong-schema, or modified export datasets before
  constructing the DuckDB relation.
- Typed query results match a direct calculation from the same export and are
  deterministic across repeated runs.
- Query result limits are validated; no host-facing raw-SQL method exists.
- Architecture checks prove core/query/runtime-protocol/MCP do not depend on
  DuckDB and no DuckDB operation launches a process or uses NDJSON.
