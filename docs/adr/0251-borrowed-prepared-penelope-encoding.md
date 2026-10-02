# ADR 0251: Borrowed prepared Penelope record encoding

Status: accepted

## Decision

Use dedicated Serialize-only wire views, following Shardline's protocol-adapter
wire DTO separation, to encode prepared Penelope record versions 1–4 without
cloning graph deltas, lineage, consequences, event vectors, or manifests.
Keep owned legacy decoding structs unchanged. Field order and exact bincode
bytes must match each existing version; no journal schema, digest, CAS,
transaction, workflow, or recovery contract changes.

Move the existing substantial adapter test suite into its dedicated child
`tests.rs`, following Shardline and this project's test-layout rule. This is
mechanical; retain every existing test.

The repository baseline shows publication dominates ingestion, but this targets
only an avoidable journal allocation. Do not claim it solves publication latency
or improves representative timings without measurement.

## Verification

All 22 Penelope integration tests and strict all-feature adapter Clippy pass.
The new serialization fixture checks exact legacy owned bytes for versions 1–4
with non-empty graph nodes, events, lineage removals, consequence retractions,
and absent/present manifests. Existing restart, crash reconciliation, lineage,
consequence, record compaction, and rejection fixtures remain intact. A fresh
full `cargo make ci` passes in 163.65 seconds, including strict workspace Clippy,
architecture/migration checks, extension isolation, all 16 backend-conformance
scenarios, and API documentation. The prior CI handle disappeared without a
retained result and is not counted as evidence. Existing allowed dependency
advisories remain unchanged.

## Representative follow-up

The existing evidence runner completed three fresh SQLite and three fresh Turso
samples on Shardline, 2026-10-01T10:32Z, rustc 1.98.1, Linux x86_64, Ryzen 9
7950X. Every initial/incremental generation passed full graph-root verification.
All six post-edit graphs retain 696 files / 190,831 nodes / 215,968 edges and the
same database byte counts as ADR-0250's baseline.

| Backend | Median initial | Median one-file incremental | Peak RSS MiB |
| --- | ---: | ---: | ---: |
| SQLite | 35.539 s | 664 ms | 2,271.3 |
| Turso | 45.916 s | 2,861 ms | 3,037.7 |

Compared with the prior run, total medians rose from 33.354/42.133 s initial and
644/2,422 ms incremental; RSS differs by only 2.2/1.2 MiB. Prepared-record CAS
stage medians moved in opposite directions across backends. These separate
three-sample runs do not isolate causation or establish a latency/memory win.
Keep the byte-compatible elimination of the explicit clone, but do not claim
it fixes publication performance or prioritize further encoder micro-tuning
from this evidence. Publication remains dominated by other measured stages.
Neither retained bytes nor peak RSS measures allocation traffic or physical
write amplification; broader workload and semantic-quality gates remain open.

Raw evidence is retained under ignored
`target/benchmark-results/repository-20261001T103234.385425Z-uncommitted/`.
