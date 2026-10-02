# ADR-0114: Give repeated CLI index transitions unique generation identities

- Status: accepted
- Date: 2026-09-29

## Context

The CLI currently derives both `GenerationId` and Penelope `IndexRunId` from
the source scan fingerprint. This makes a generation identify a snapshot, not
an accepted transition. After indexing snapshot A, then B, then A again, the
second A reuses the first A's generation and run IDs even though its expected
base is B. Penelope correctly rejects that as reuse of an idempotency key for a
different request, and the history model cannot represent the second A as a
distinct accepted transition.

The same-snapshot retry case must remain idempotent, including when an earlier
publication succeeded but the caller did not receive its receipt. Generation
history must also remain cheap to consult at the current head; indexing must
not read/replay the full retained history just to derive an identity.

## Decision

1. Preserve the existing content-derived `source-index-v1` generation ID for
   the first accepted CLI snapshot and as a compatibility identity for
   snapshots already indexed by older CLI versions.
2. For a new CLI snapshot when a generation is already current, derive the
   transition generation ID with a domain-separated hash over the current base
   generation, extractor/configuration fingerprint, and canonical source scan
   fingerprint. Continue deriving the Penelope run ID from that resulting
   generation ID. Thus the same transition request from the same base is
   retry-stable, while returning to a historical snapshot from a different
   base creates a distinct accepted generation.
3. Before publishing, treat an identical scan as already indexed when either
   its legacy `source-index-v1` identity equals the current generation, or the
   current generation is the transition derived from the parent recorded in
   its latest accepted delta and this scan's fingerprint. Return the current
   generation without creating a no-op transition. Configuration/extractor and
   enabled resolver fingerprints remain inputs, so changed indexing semantics
   do not take this fast path.
4. Add a `GraphStore` read for the current generation's history entry. InMemory
   and File use their in-memory ordered history; SQLite and Turso query the
   exact current generation row. Do not load the complete history to discover
   the current parent.
5. Keep `GenerationId`, `IndexRunId`, manifests, storage schemas, and wire DTOs
   unchanged. This is a CLI identity-derivation correction; explicit Engine
   callers remain responsible for supplying stable transition identities.

## Alternatives considered

- Keep generation IDs content-addressed and change only run IDs: rejected
  because generation IDs are unique keys in accepted history and cannot name
  two parented transitions.
- Add a new caller-supplied snapshot fingerprint field to public graph
  contracts or manifests: rejected for this fix because deterministic CLI
  derivation and a current-entry lookup solve the case without a DTO or schema
  migration.
- Use random IDs per invocation: rejected because retries after interruption
  would not identify the same prepared transition.
- Read all generation history to find the parent: rejected because indexing
  work would grow with retained history depth.

## Consequences

- A → B → A is represented by three distinct accepted generations; the final
  A has B as its expected base and its own stable Penelope run ID.
- Re-indexing the current snapshot creates no additional generation. A retry
  of an uncommitted transition derives the same generation and run IDs.
- Existing first-generation and same-content identities remain compatible;
  post-bootstrap transition IDs use a new domain tag.
- Generation ID calculation remains a host-level concern and does not change
  the meaning of graph roots or fact identities.

## Verification

- CLI integration coverage indexes A → B → A and verifies three unique,
  parent-linked generations and successful publication.
- Re-indexing the final A leaves the current generation and history length
  unchanged.
- The identity derivation includes the existing extractor and enabled resolver
  fingerprints; their configuration-change integration tests continue to
  verify those changes are not mistaken for an unchanged scan. Existing
  Penelope recovery tests cover deterministic replay of prepared requests.
- Cross-backend conformance verifies current history-entry lookup for
  InMemory, File, SQLite, and Turso. SQLite and Turso fetch the entry using the
  unique generation key instead of loading retained history.

## References

- [ADR-0025: Generation configuration and producer fingerprints](0025-generation-configuration-and-producer-fingerprints.md)
- [ADR-0026: Current projection and first-class history](0026-current-projection-and-first-class-history.md)
- [ADR-0058: Extractor identity and reindex invalidation](0058-extractor-identity-and-reindex-invalidation.md)
