# ADR-0153: Provenance-classified context items

- Status: accepted
- Date: 2026-09-30

## Decision

Preserve Graphify's useful explicit extracted/inferred distinction using
SyntaxMesh's existing typed provenance, rather than introducing another trust
taxonomy. Context pack schema v2 adds `evidence_class: Option<EvidenceClass>` to
each item. Newly compiled items always carry the originating fact's class:
graph paths use their edge provenance; signatures, summaries, and source
snippets use their node provenance. Missing provenance fails compilation rather
than defaulting to SourceFact. The optional default permits legacy v1 payloads
to deserialize with unknown classification; it is not a new-producer fallback.

For a snippet selected through a semantic claim/concept, SemanticInference
describes the selecting fact, not the authenticity of the quoted bytes. Source
hash/span checks still apply independently; neither exact quotations nor this
class establishes entailment. RuntimeObserved also is not proof of causality.

Resolve only candidate-referenced provenance through the existing keyed store
port once per compilation, on the pinned generation. Keep DTO and query crates
free of host, model, transport, and workflow dependencies. No graph/store schema,
fact identity, inference prompt, cache key, or publication workflow changes.
Existing serialized-budget packing includes the new field before admission.

## Verification

Verify node and edge classification, semantic and source distinction, legacy
unknown decoding, current schema round-trip, exact budget accounting, and
cross-backend context conformance. Reuse the joint-document restart fixture.

Legacy unknown decoding and all eight class round-trips pass. Joint-document
fixtures compare every item's class with its actual node/edge provenance and
require both source and semantic classes, including after File-store restart.
Full `cargo make ci` passes, including exact tokenizer accounting, cross-backend
context conformance, strict Clippy, and architecture dependency checks.
