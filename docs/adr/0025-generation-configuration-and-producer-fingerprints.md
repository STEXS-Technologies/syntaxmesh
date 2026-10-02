# ADR-0025: Populate generation configuration and producer fingerprints

- Status: accepted
- Date: 2026-09-27

## Context

`GenerationManifest` already reserves `configuration_hash` and
`extractor_set_hash`, but both stores emitted all-zero values. This discarded
producer versions already carried by fact provenance and did not identify the
fixed indexing/resolution policy used by this first implementation.

## Decision

For the current release, the engine exposes no caller-configurable parsing or
resolution options. `configuration_hash` therefore fingerprints a canonical,
versioned descriptor of the current default indexing/resolution policy. Any
future configurable option must become part of a versioned configuration value
whose canonical encoding is fingerprinted; silently keeping the same hash when
policy changes is not allowed.

`extractor_set_hash` fingerprints the sorted, deduplicated
`(producer_namespace, producer_version)` pairs referenced by nodes or edges in
the accepted generation. That includes language extractors and extension or
runtime producers that contributed canonical facts, and excludes unreferenced
historical provenance. Both backends use the same domain-separated canonical
hash helper. This decision changes manifest population only; it does not
change `GraphDelta`, Penelope's durable record encoding, graph facts, or IDs.

## Consequences

- Same facts with the same provenance producers yield the same producer-set
  fingerprint across all stores and restarts.
- Removing the last fact from a producer removes that producer from the next
  manifest's active producer set.
- A rebuild after an extractor upgrade must actually regenerate affected facts
  for their provenance versions to change; automatic extractor-version
  invalidation remains a separate indexing contract.
- Configuration hashing is explicitly a v1 fixed-default fingerprint until a
  public configuration model is introduced.
