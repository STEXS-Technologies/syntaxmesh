# ADR-0088: Stream and expose bounded historical neighborhoods

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0087 added the bounded exact-generation historical neighborhood to the
runtime-neutral Rust query API. The feature is not practically discoverable to
CLI users until it has an interchange stream and CLI entry points. Existing
`TemporalRecord` item/footer conventions and the paired File/Turso command
pattern already provide those boundaries.

## Decision

1. Add one query serializer that emits generation-tagged node records with
   shortest discovered depth, edge records with shortest discovered depth,
   and a final footer with node/edge counts, scanned-incidence count,
   serialized item-byte count, and truncation status. Node records carry full historical `Node` payloads;
   edge records retain original direction and provenance identity.
2. Give this operation its own schema version, independent of unrelated
   temporal record families. The stream is finite and budget-bounded; it does
   not offer continuation because the traversal limits define the complete
   request boundary.
3. Add matching Rust CLI commands `neighborhood-at` and
   `neighborhood-at-turso`, taking a pinned generation, comma-separated seed
   IDs, and explicit hop/node/edge/scanned-incidence/output-byte caps. Both commands call
   the same engine/query operation and emit identical NDJSON for equal stores.
4. Preserve the strict Rust-only execution boundary. These commands do not
   launch analyzed runtimes, and they do not add generation-range, calendar,
   relation, evidence, or causality semantics.

## Consequences

- The historical traversal becomes usable from the supported local CLI and
  remains available to embedded Rust callers through the same query layer.
- The wire format is additive and versioned independently from temporal
  history pages and timelines.
- Serialized provenance payload objects are not duplicated in this stream;
  node/edge payloads retain their provenance IDs, which can be resolved using
  the corresponding fact/history query APIs.

## Verification

Round-trip each new item/footer record; validate seed and bound parsing; assert
File/Turso output equality and deterministic depth/order; check counts and
truncation; verify help documents both commands and the Rust-only scope.
