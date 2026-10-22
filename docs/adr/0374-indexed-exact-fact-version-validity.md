# ADR-0374: Indexed exact fact-version validity lookup

- Status: Accepted
- Date: 2026-10-22

## Context

The v5 `fact_version_retirement` query resolves the exclusive end generation
for one exact `(fact, valid_from)` pair. Its current implementation obtains that
coordinate by loading and scanning every retained version for the fact. SQLite
and Turso already store fact versions under the composite primary key
`(fact_kind, fact_id, valid_from_sequence)`, so history depth for one fact need
not affect this point lookup.

## Decision

Add a small store-port `FactVersionValidity` result and
`GraphStore::fact_version_validity` operation keyed by the exact fact identity
and start generation. InMemory/File use the retained-history reference path;
SQLite/Turso seek the existing composite primary key and resolve the optional
exclusive end generation from generation history. A non-null end sequence with
no retained end generation is an integrity error, not an active version.

Route `Query::fact_version_retirement` through this point lookup. Keep the
selected-generation cutoff check and direct `Updated`/`Removed` event-membership
validation. The operation does not infer semantic resolution or causation.
No schema migration is needed.

## Consequences

- Retirement lookup work is independent of the number of versions retained for
  the same fact on SQLite/Turso; the reference stores remain simple correctness
  oracles.
- The store port gains an additive method and value type. No existing serialized
  DTO, schema, or persisted record changes.
- Tests must cover exact version, active/unknown versions, malformed closure
  metadata, and query-plan use of the existing identity/start primary key.
