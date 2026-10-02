# ADR-0028: First generation-based temporal query contracts

- Status: accepted
- Date: 2026-09-27

## Context

The source of truth requires typed timeline queries such as `History(entity)`,
`StateAt(entity, generation)`, `GraphAt(generation)`, and
`ChangedBetween(a, b)` (§237), and says public DTOs should hide internal
storage (§276). ADR-0027 established temporal fact versions and checkpoints,
but the first implementation had no application-level history or diff query.
The current core ontology versions graph nodes and edges; claims, flows,
contracts, calendar time, and full observation/acceptance bitemporality are
not yet implemented.

## Decision

Expose two generation-based operations through the query service:

- `history(node_id)` returns the retained node payload versions with inclusive
  `valid_from` and exclusive `valid_until` generation boundaries. A missing
  `valid_until` means the node remains present in the current retained history.
- `changed_between(from, to)` treats `from` as exclusive and `to` as inclusive,
  requires both generations to be retained on the same parent chain, and
  returns one manifest/delta pair per accepted transition. The initial anchor
  is not a transition and cannot report facts before its generation.

Durable stores answer node history from the identity/version interval index
and range diffs from the generation-sequence index. Reference stores derive
these results from retained deltas. Public timeline output uses a versioned
`TemporalRecord` NDJSON DTO with generation IDs, domain IDs, and typed fact
payloads; no SQL sequence numbers escape the store. Ordinary graph export
schema remains unchanged.

## Consequences

- `History` is O(log V + K) over the node's versions, where K is its returned
  version count. `ChangedBetween` is O(log H + R), where R is the transitions
  returned. Neither depends on replaying the whole graph history in durable
  backends.
- `StateAt(node, generation)` already has an indexed single-version store
  operation. Durable `GraphAt` selects a structurally shared generation root
  and validates the resulting manifest root; it does not scan prior versions
  or use checkpoints on its normal read path (ADR-0030). It remains
  output-size-bound.
- The CLI exposes node history and generation queries; calendar-time
  predicates, semantic entity kinds beyond canonical files/provenance/nodes/
  edges, implicit cascade attribution, and bitemporal state selection remain
  future work and must not be implied. Canonical fact-family history is
  specified in ADR-0033.
- ADR-0031 decides the separate modeled-validity, observation, and
  acceptance-time axes plus historical interpretation modes. ADR-0033 adds
  canonical fact-history DTOs with these coordinates where available; it does
  not implement calendar-time predicates or bitemporal state selection.
