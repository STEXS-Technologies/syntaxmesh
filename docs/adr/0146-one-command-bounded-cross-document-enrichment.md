# ADR-0146: Opt into bounded cross-document enrichment in one command

- Status: accepted
- Date: 2026-09-30

## Decision

Add `--semantic-cross-document`, requiring `--semantic`, to opt into additional
AI work in the same indexing command. Keep ordinary document extraction and its
cache unchanged. After it completes, use the existing generation request builder
for bounded source-backed requests containing at least two distinct documents.
Expose the pure request's derived `source_document_count` for this selection.

Use the existing provider, Penelope completion records, and model/prompt/config
identity. Compound input hashes include their complete supporting source content.
Changing a document invalidates affected compound inputs without discarding
unchanged document extraction. No claims or graph rows are used as source text:
the second layer sees original source chunks and authored heading context.

Merge both layers through `SemanticOutput::merge`, validate against the document
layer's complete generation-pinned source request, then publish one semantic
replacement. A second-layer failure publishes no partial semantic replacement;
completed work remains cached. The flag also works with explicit offline cache
reuse. Extra inference is opt-in rather than an unannounced cost increase.

## Limits

This is bounded joint source extraction, not corpus-wide synthesis, automatic
entity aliases, or proof of model entailment. Only documents fitting in one
generation batch can jointly support a claim. Existing deterministic partition,
byte/chunk bounds, provider policy, and revision checks remain unchanged.
No database migration, runtime, provider SDK, new scheduler, or core dependency
is added. Both layers currently use the same extraction prompt identity.

## Verification

Verify two layers, compound evidence ownership, cache reuse, a document edit,
historical isolation, failed compound inference without partial publication,
and option validation on File and verified Turso. Real-model quality remains open.

Four CLI fixtures pass: initial runs use two prompts for three cache units;
unchanged repeats reuse all three; a document edit reuses the untouched document
and recomputes two requests; a failed compound step retains two document cache
units and retries only the compound request. Exact hash-verified claim review
retains both quotations on the joint claim. Final repeats use strict offline
mode with the provider closed. Full real-model quality evidence remains absent.
