# ADR-0045: Consequence neighborhood NDJSON

Status: Accepted

Implementation status: Implemented in temporal NDJSON schema v7 and the
`consequence-neighborhood[-turso]` CLI commands.

## Context

ADR-0042 defines evidence-backed consequence assertions, ADR-0043 adds indexed
fixed-generation endpoint reads, and ADR-0044 adds bounded weakly connected
traversal. These capabilities are currently usable only through the embedded
query API. Operators and runtime-neutral consumers need a stable interchange
representation that preserves the selected generation and makes truncation
explicit.

## Decision

- Add consequence edge and neighborhood footer records to the versioned
  `TemporalRecord` NDJSON stream and increment its schema version to 7.
- Each edge record includes its shortest discovered hop depth and complete
  typed edge (including evidence, derivation, provenance, and original
  direction). The footer includes the pinned generation, endpoint/edge counts,
  and truncation status.
- Add `consequence-neighborhood[-turso]` CLI commands. Arguments identify the
  store, generation, one seed endpoint, hop limit, endpoint cap, and edge cap.
  A seed is either a change-event ID or an exact fact-version reference
  (fact-kind, fact-ID, valid-from generation).
- CLI output is deterministic NDJSON from the query API; it does not replay
  deltas, infer causality, or alter canonical state.
- This does not add resumable traversal cursors, all-seed CLI syntax, or an
  import format. Those require separate contracts.

## Consequences

Consumers can persist and process bounded historical consequence neighborhoods
without depending on Rust types. The stream is an export, not a restorable
backup. The generation in the footer binds all records to one historical
conclusion.
