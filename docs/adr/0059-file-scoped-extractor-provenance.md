# ADR-0059: File-scoped extractor provenance identities

- Status: accepted
- Date: 2026-09-28

## Context

The Rust, Python, TypeScript, and JavaScript extractors derived provenance IDs
from producer namespace and content hash alone. Two different files with
identical bytes therefore received the same ID, even though each provenance
record names a different source file. A multi-file index then rejected the
delta as containing duplicate identities. The repository-indexing benchmark
reproduced this with a 168-file sibling Rust repository.

## Decision

1. Derive source-extractor provenance IDs from producer namespace, stable
   `FileId`, and content hash. Provenance is specific to a producer's evidence
   about one source-file version; equal bytes at different paths are distinct
   evidence records.
2. Bump each affected extractor's semantic producer version so the indexer
   re-extracts unchanged files previously indexed with the colliding identity
   scheme. Rust advances from `0.2.0` to `0.2.1`; Python, TypeScript, and
   JavaScript advance their extractor semantic version from `1` to `2`.
3. Preserve source content hashes and fact identity rules. This changes only
   provenance identity; it does not claim that identical content at different
   paths has distinct semantic declarations when a language pack's own fact
   identity intentionally says otherwise.

## Alternatives considered

- Keep content-only provenance and deduplicate colliding records: rejected;
  one record cannot truthfully name two source files, and choosing either
  source would lose evidence.
- Change only the Rust extractor: rejected; the same derivation is used by all
  bundled source extractors and must have one consistent contract.
- Include the producer version in the ID: unnecessary because extractor
  provenance is upserted by stable per-file identity as its version changes;
  the producer version already triggers re-extraction.

## Consequences

- Equal-content files can be indexed together without duplicate provenance
  identities, and their source binding remains exact.
- Existing stores re-extract files lazily as each unchanged file is next
  indexed, through the established producer-identity invalidation path. No
  database schema migration is needed; prior provenance remains valid history.
- Tests must cover equal bytes at distinct paths for every bundled extractor,
  plus unchanged-file re-extraction after an extractor semantic-version bump.
