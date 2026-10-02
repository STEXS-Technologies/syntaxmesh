# ADR-0095: Filter consequence traces by assertion evidence class

Status: Accepted

## Context

[ADR-0093](0093-generation-range-consequence-traces.md) requires exact-set
evidence-class filtering while rejecting ordinal trust interpretations.
`ConsequenceEdge` identifies the producer provenance asserting the edge; its
supporting fact-version references may have different provenance and classes.
Loading every provenance row for each trace would turn an endpoint-indexed
query into work proportional to the repository's total provenance history.

## Decision

- Add `included_evidence_classes` to `ConsequenceTraceRequest`. An empty set
  includes every assertion; otherwise an edge is eligible only when the
  `EvidenceClass` of its own producer provenance is a member of the exact set.
  Supporting fact provenance remains attached and is not interpreted by this
  filter. No class ordering or trust threshold is implied.
- Add a generation-validated `GraphStore::provenance_for_ids` read port. Its
  compatibility default may filter the reference backend's provenance result;
  SQLite and Turso override it to use the existing provenance primary key and
  read only requested IDs, in bounded batches. The operation adds no schema or
  dependency to core, DTO, or query crates.
- Resolve producer classes per bounded consequence page, then filter before
  adding reachable states or hops. A missing referenced provenance row is a
  store-integrity error, not a silent non-match.
- Expose an optional exact `--evidence-classes` set on both CLI trace commands,
  using the same semantics as the query request. Omitting it includes all
  classes; do not silently assign a default class policy.

## Consequences

Consumers can select assertion classes without losing the complete underlying
evidence payload. Durable stores do not scan the provenance table or replay
history for filtered traces. The filter changes eligibility, not the
chronology, validity, endpoint, or causal interpretation of retained hops.

## Verification

- Test empty, singleton, multi-class, non-matching, and missing-provenance
  behavior on reference and durable stores, including CLI parsing and output.
- Verify SQLite/Turso provenance reads are keyed by requested IDs and
  differential trace results agree across backends.
