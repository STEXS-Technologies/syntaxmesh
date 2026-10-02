# ADR-0035: Label temporal query interpretation in public records

- Status: accepted
- Date: 2026-09-27

## Context

SyntaxMesh's source of truth (§238) requires historical results to distinguish
what the engine concluded using evidence and semantics available at the time
from a retrospective evaluation of an old system state under current
semantics. ADR-0031 assigns existing generation-scoped queries to historical
conclusion mode, but the public NDJSON records currently expose only the
generation or time coordinate. Consumers therefore have to infer whether a
record is a state conclusion or a timeline of raw evidence.

The current API supports generation history, engine acceptance timelines, and
producer-observation timelines. It does not implement current-semantics
retrospective evaluation or full bitemporal state selection. Adding a label
must not imply either capability.

## Decision

- Add a public `TemporalQueryMode` to versioned interchange records.
- Label graph snapshots, generation changes, node history, and typed fact
  history as `historical_conclusion`. These records are scoped to the accepted
  generation/state selected by the caller and do not re-run newer semantics.
- Label acceptance-time results as `acceptance_timeline` and producer event
  time results as `observation_timeline`. These are ordered evidence/metadata
  listings, not graph-state conclusions.
- Reserve `current_semantics_retrospective` as a representable mode for a future
  query API; do not emit it until the required old-state/current-semantics
  evaluation exists.
- Bump the graph export, temporal export, and acceptance timeline schema
  versions because the serialized contract gains a required field. Every
  emitted row and footer repeats the mode so records remain interpretable when
  separated from their stream header.
- Do not add `StateAtTime` or `KnownAt`. Calendar/bitemporal state selection
  still requires an explicit intersection of modeled validity and acceptance
  knowledge time; producer observation time alone cannot select graph state.

## Alternatives considered

- **Rely on command names or documentation:** rejected because exported rows
  can be detached from their command and consumed by generic tooling.
- **Call all temporal records historical conclusions:** rejected because
  observation and acceptance timelines return ordered metadata/evidence, not a
  reconstructed graph conclusion.
- **Add retrospective mode before implementing it:** rejected because a mode
  label must describe executed semantics, not user intent.

## Consequences

- Consumers can distinguish historical conclusions from timestamp-indexed
  evidence timelines without reading command-specific documentation.
- Graph/query facts and identities are unchanged; only interchange schemas
  advance.
- Typed in-process query methods remain generation-bound and use
  historical-conclusion semantics. A future current-semantics query must use a
  distinct API and explicitly emit the retrospective label.
- Serialization fixtures and CLI/embedded export parity must cover the mode
  field and updated schema versions.
