# ADR-0057: Model class declarations with an explicit node kind

- Status: accepted
- Date: 2026-09-28

## Context

The v0.1 language plan requires Python and TypeScript/JavaScript packs. The
current core `NodeKind` has `Struct`, `Enum`, and `Trait` but no class kind.
Python classes and JavaScript/TypeScript classes are not structs, and
mislabeling them would make the graph's typed facts false. Treating them as
extension-specific opaque values would prevent language-independent clients
from querying a common object-oriented declaration concept.

Node kinds are serialized in graph snapshots and compact persistent-fact
pages. Bincode encodes enum variants by ordinal, so changing the order would
reinterpret already stored facts.

## Decision

- Add `NodeKind::Class` as the final enum variant. Existing variants retain
  their serialized ordinal; new class facts use the appended ordinal.
- Class declarations are source facts with normal file ownership, provenance,
  source spans, and deterministic stable IDs. The core does not encode
  language-specific inheritance or metaclass semantics in this node kind.
- Include classes among definitions eligible for conservative reference
  resolution, enabling class-constructor references to resolve without
  changing the resolver's confidence policy.
- Keep producer/parser versioning in each language pack's `Provenance`; parser
  upgrades remain an explicit identity/configuration invalidation concern.

## Alternatives considered

- Reuse `Struct`: rejected because the fact would be semantically incorrect
  for dynamic-language classes.
- Encode class declarations as `External`: rejected because these are native
  source declarations, not external or extension-owned facts.
- Add `Class` in the middle of `NodeKind`: rejected because it would shift
  bincode variant ordinals for all subsequent existing variants.
- Introduce a fully language-neutral `TypeDeclaration` hierarchy now:
  deferred; current language packs need a precise class label, not a new type
  system for inheritance and type semantics.

## Compatibility and consequences

- Old snapshots/pages containing existing variants remain readable by the new
  version because their ordinals are unchanged. New data containing
  `NodeKind::Class` is not readable by older binaries; downgrade after
  publishing class facts is unsupported, as with other forward schema changes.
- Public DTO consumers must tolerate the additive `class` variant or pin their
  DTO schema. No table migration is required because node kinds are carried in
  serialized fact values and the appended bincode variant preserves old data.
- Regression tests must round-trip old enum variants and new class facts,
  verify stable class IDs across line-only edits, and confirm constructor
  references resolve only to a unique class candidate.
