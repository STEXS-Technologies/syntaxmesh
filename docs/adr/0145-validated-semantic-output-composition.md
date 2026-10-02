# ADR-0145: Compose semantic layers from validated durable output

- Status: accepted
- Date: 2026-09-30

## Context

The existing engine can ground a joint claim across documents, but its durable
batch API immediately converts validated output into graph facts. Composing
document extraction and bounded synthesis would otherwise require decoding graph
facts back into claims or introducing a second graph merge implementation.

## Decision

Add an output-returning variant to the existing Penelope batch workflow and
engine API. It returns the combined source request and validated `SemanticOutput`
after the same durable prepare, evidence validation, completion, and cache reuse
path. Keep the existing fact-returning method as a wrapper that converts this
result with `SemanticRequest::into_fact_batch`.

No publication occurs in either enrichment API. Callers compose outputs using
the existing `SemanticOutput::merge`, then validate the merged result against
its complete pinned source request before ordinary atomic semantic replacement.
Do not combine overlapping request lists just to merge outputs: physical source
chunks must remain unique in the final validation request.

This adds no storage schema, runtime, transport, provider SDK, graph merge
algorithm, or core dependency. Existing prepared and completed records keep
their formats and cache identities. The additive engine/workflow methods use
existing pure semantic types, not host DTOs.

## Verification

Exercise the output-returning path with the existing joint-document fixture,
compare cached results with a deliberately failing provider, convert the
returned request/output to facts, and preserve its existing grounding, retry,
publication, and historical-isolation assertions. Run existing workflow tests
to verify the fact-returning wrapper remains equivalent.

The joint-document fixture also runs separately cached document extraction and
merges its two claims with the joint claim through `SemanticOutput::merge`.
Converting against the document phase's complete source request produces three
claims and four support edges. This tests the reusable composition boundary;
automatic CLI synthesis and real-provider quality are not claimed.
