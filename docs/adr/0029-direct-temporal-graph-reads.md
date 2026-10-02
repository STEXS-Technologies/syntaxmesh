# ADR-0029: Serve historical graph reads from temporal fact indexes

- Status: accepted
- Date: 2026-09-27

## Context

ADR-0027 established temporal fact versions as the historical query projection,
but the first durable `GraphAt(generation)` implementation still loaded a
checkpoint and replayed a bounded delta tail for each request. Bounded replay
does not grow with total history depth, but it repeats transition work and
makes checkpoints part of the read path. That is the wrong default for a
time-depth query surface. SyntaxMesh must make selecting a historical depth
independent of the number of intervening generations, while keeping result
production proportional to the requested graph.

## Decision

- Resolve the requested generation to its retained sequence, then read the
  active file, provenance, node, and edge payload versions directly from the
  durable temporal-fact projection.
- Verify the resulting snapshot against the retained generation manifest root
  before returning it. Corrupt or incomplete fact projections therefore fail
  closed instead of silently producing a partial historical graph.
- Keep append-only deltas as canonical audit/rebuild evidence and keep periodic
  graph checkpoints as recovery, migration, and index-rebuild accelerators.
  They are not used to answer ordinary durable `GraphAt` requests.
- Preserve the reference-store replay implementation as the simple conformance
  oracle. Durable point-state lookups use identity/version indexes; broad
  historical queries remain output-sized and must be scoped or streamed.

“O(1) over history depth” refers to generation selection and avoiding replay
or traversal of intervening generations. It is not a claim that arbitrary
database operations are literally constant-time, that a full graph can be
returned in constant time, or that temporal range predicates have a guaranteed
constant-cost plan. Point lookups are index-depth plus result-size; `GraphAt`
is at least O(graph output size). Query-cost benchmarks must keep output size
fixed while increasing both history depth and fact-version churn.

## Consequences

- Normal durable historical reads no longer replay even a bounded delta tail.
- Fact-version interval indexes are a correctness-critical query projection;
  publication, migration, and integrity checks must maintain them atomically
  with accepted generation history.
- Checkpoint corruption cannot affect a normal `GraphAt` read, but checkpoint
  validation remains necessary for recovery and rebuild operations.
- The current SQLite/Turso temporal interval scan is an initial implementation,
  not the final layered temporal index. If fixed-result cost grows with retained
  version count, replace the range scan with a structurally shared generation
  root or an equivalent interval-stabbing index without changing public DTOs.
- Future valid-time and observed/accepted-time dimensions require explicit
  bitemporal indexes and query modes; generation depth is not wall-clock time.

## Superseded for full graph snapshots

[ADR-0030](0030-structurally-shared-generation-roots.md) supersedes the
interval-scan `GraphAt` implementation described above. Temporal fact indexes
remain the point-history and timeline-query projection; durable full-snapshot
reads now traverse the structurally shared immutable generation root.
