# ADR 0250: Indexed Rust declaration ordinals

Status: accepted

## Decision

Reuse the ordered occurrence-counter pattern already used by Rust imports and
implementation headers, and the keyed counter pattern in Shardline's query
admission (`shardline-hub-api/src/parquet_preview.rs`). Keep one extraction-local
map keyed by declaration kind and qualified name instead of scanning every
previous node for every declaration. Share this collector through function,
module, impl, and trait declaration recursion.

Preserve the exact existing first-name / `name#ordinal` identity encoding and
source traversal order. Seed counters from the existing file-module node.
No producer version, public contract, dependency, storage format, or migration
changes: extraction facts must remain identical. This removes the quadratic
duplicate-count scan, not all ingestion costs; no end-to-end latency claim is
made without representative measurement.

## Verification

All 24 extractor tests and strict extractor Clippy pass. A differential fixture
compares duplicate and cross-kind identities with the previous node-scan oracle,
including repeated nested functions, trait methods, and modules. A second fixture
extracts 1,000 distinct outer functions plus 1,000 local functions and verifies
declaration/containment counts. All 12 File/verified-Turso lifecycle tests pass.
Full `cargo make ci` passes in 230.47 seconds, including workspace strict Clippy,
architecture and migration registries, extension isolation, all 16 backend
conformance scenarios, and API documentation. Existing allowed dependency
advisories remain unchanged. The counter map is dropped immediately after
declaration collection, before resolution and publication.

## Representative current-state evidence

The existing Shardline-style evidence runner completed three fresh samples per
SQLite/Turso backend on Shardline's 696 Rust files (14,669,297 source bytes),
2026-09-30T23:05Z, rustc 1.98.1, Linux x86_64, Ryzen 9 7950X. Every initial and
incremental generation passed the runner's full graph-root check. All six runs
reported 190,831 nodes and 215,968 edges after the in-memory one-file edit; the
source checkout was not changed.

| Backend | Median initial | Median one-file incremental | Retained DB bytes | Peak RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| SQLite | 33.354 s | 644 ms | 1,030,746,112 | 2,273.5 |
| Turso | 42.133 s | 2,422 ms | 1,206,841,448 | 3,038.9 |

Initial delta preparation (including reads, extraction, and resolution) measured
1.195/1.212 s for SQLite/Turso, versus 32.151/40.813 s Penelope publication.
Publication remains the dominant measured stage. Medians are independent
samples and are not additive timing decompositions. This is a current-state
baseline, not an isolated before/after measurement: recent Rust source-fact and
resolution changes altered graph counts. It establishes neither a collector
speedup, compiler-level semantic correctness, a latency SLA, nor physical write
amplification. Broader repository coverage remains open.

Raw logs, samples, metadata, database accounting, and summary are retained under
ignored `target/benchmark-results/repository-20260930T230552.006505Z-uncommitted/`.
