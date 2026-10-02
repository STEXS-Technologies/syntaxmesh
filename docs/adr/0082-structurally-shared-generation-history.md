# ADR-0082: Structurally share retained reference-store generation history

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0081 made canonical maps and derived indexes structurally shared, but a
feature-gated clone profile on the same 696-file Shardline SQLite benchmark
still attributed 30.875 ms to `InMemoryGraphStore.history.clone()` for a single
incremental publication. The initial retained `GenerationHistoryEntry`
contains the large canonical delta, so cloning the outer `Vec` recursively
copies its fact vectors even when the new candidate only appends one entry.
The persistent collections from the already adopted `imbl` dependency provide
the same shared-prefix property without a SyntaxMesh-specific history overlay.

## Decision

1. Store the private reference-store history sequence as `imbl::Vector`.
   Candidate clones share existing immutable chunks; appending a generation
   creates a branch-local suffix rather than deep-cloning retained deltas.
2. Keep public `GraphStore::generation_history` and legacy snapshot DTOs as
   `Vec`. Convert only at those explicit materialization/compatibility
   boundaries; full history reads necessarily copy their requested output.
3. Preserve the existing Bincode sequence encoding. Add bidirectional
   `Vec`/`Vector` serialization tests and keep legacy version-1 snapshot types
   unchanged. No File snapshot version, public DTO, SQL schema, or canonical
   root changes.
4. Keep lineage/consequence vectors unchanged until their clone cost is
   separately measured. Do not generalize the representation change without
   evidence.
5. Keep the change only if clone isolation, snapshot compatibility, complete
   store/cross-backend tests, and the publication-scaling benchmark pass, with
   a reduced history-clone stage and no unacceptable initial-index regression.

## Consequences

- Candidate history forks share retained entries while each candidate appends
  its own accepted transition.
- APIs that return the entire generation history remain proportional to the
  requested history output.
- Persisted history bytes, temporal semantics, append order, and legacy
  readability remain unchanged.

## Verification

- Store tests pass, including the new `Vec`/`Vector` Bincode byte-identity test,
  legacy v1 snapshot migration, restart, and candidate-failure fixtures.
- SQLite migration/publication tests (36/36) and Turso backend conformance
  tests (10/10) pass with the persistent history representation.
- On the three-run Shardline SQLite evidence workload, candidate-store clone
  median was 18 microseconds versus 120.398 ms before structurally sharing the
  canonical maps, indexes, and history. The feature-gated group profile reports
  the retained-history clone at 1–6 microseconds rather than 30.875 ms before
  this ADR. These timings are host/workload evidence, not guarantees.
- Initial/incremental real-repository medians were 29.586 s / 622 ms versus
  29.171 s / 711 ms in the prior run bundle; retained DB size was unchanged.
  Evidence is recorded in
  `target/benchmark-results/repository-20260929T010710.120719Z-uncommitted/`.
