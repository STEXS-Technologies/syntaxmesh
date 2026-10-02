# ADR-0033: Expose typed history for canonical graph facts

- Status: accepted
- Date: 2026-09-27

## Context

The source-of-truth requires temporal continuity for nodes, edges, runtime
observations, and other durable fact families (§236), plus typed `History`
queries (§237). SQLite and Turso already retain interval versions for files,
provenance, nodes, and edges in `syntaxmesh_fact_versions`; however, the public
history surface returns nodes only. As a result a historical caller cannot
directly retrieve the prior relationship or evidence payload that explains a
node's temporal state.

## Decision

- Add a runtime-neutral `FactRef` discriminated by canonical fact family:
  file, provenance, node, or edge. Each variant carries that family's stable
  typed ID; do not expose SQL fact-kind integers or sequence numbers.
- Add `GraphStore::fact_history(FactRef)` returning ordered typed fact-version
  payloads with `[valid_from_generation, valid_until_generation)` intervals.
  Preserve every retained version, including versions closed by deletion or
  replacement; do not synthesize a live version for an absent fact.
- Each version reports optional producer `observed_at` and engine `accepted_at`
  independently. Unknown legacy acceptance times and facts without an
  observation timestamp remain `None`; neither is inferred from generation
  order. Observation time is populated only where the durable fact metadata
  carries the producer timestamp (currently runtime observations).
- Durable stores read the existing fact-version identity/interval index and
  join interval endpoints to retained generation IDs. Reference stores derive
  the same logical result from retained generation snapshots/deltas.
- Expose versioned temporal DTO variants for the four canonical fact payloads.
  Keep the existing node-history API/record shape compatible; callers may use
  the generic query for any fact family. Generation IDs remain validity
  boundaries, not calendar timestamps.
- This API reports canonical fact versions only. It does not infer causality,
  collapse related facts into a ChangeEvent, or reinterpret evidence under
  current semantics. Those require separate contracts and provenance-backed
  lineage indexes.

## Alternatives considered

- **Expose only node history:** rejected because it leaves edges and evidence
  unavailable to historical graph traversal despite already-versioned storage.
- **Return untyped serialized payload bytes:** rejected because consumers then
  depend on private storage encoding and cannot exhaustively handle fact kinds.
- **Build a complete historical graph for every entity-history request:**
  rejected because indexed identity/interval lookup should be proportional to
  returned versions, not all graph facts or history depth.

## Consequences

- The temporal query API becomes a graph-fact history surface rather than a
  node-only endpoint; relationship/evidence views can be assembled from typed
  histories without replaying prior deltas in durable stores.
- The first contract covers the four canonical persisted fact families only.
  Claims, ChangeSets/ChangeEvents, contracts, flows, semantic reinterpretation,
  and consequence lineage remain distinct future fact models.
- Cross-backend fixtures must cover insert, replacement, deletion, retained
  intervals, deterministic order, serialization, and restart. Query cost is
  indexed lookup plus output size, not strict O(1).

Implementation status: `FactRef`, `FactPayload`, and `FactHistoryVersion` are
implemented in the core/store boundary, including separate optional
observation/acceptance timestamps. SQLite and Turso query the existing
fact identity/validity index; File/InMemory derive results from retained
deltas. `Query::fact_history` returns versioned temporal records. Shared
backend-conformance and DTO round-trip fixtures cover all four fact families;
the CLI and embedded API both expose the operation. The CLI selects a fact
family and stable ID explicitly.
