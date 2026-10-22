# ADR-0368: Bounded Source-Span Lexical Planner

Status: accepted

Date: 2026-10-22

## Context

The opt-in lexical planner indexes canonical node labels and, where configured,
paths. Context packing later reads source text only for already-selected graph
nodes. Consequently, identifiers and rationale present in a source span but
absent from its node label cannot help select that node. Whole-repository
retrieval evaluations show this remains a v0 quality gap; target-specific seed
quotas and larger graph/token limits have not been accepted as fixes.

## Decision

1. Add an additive, opt-in planner builder that augments existing exact and
   stemmed lexical postings with tokens from source-backed declaration and
   document-content spans. Preserve ordinary builders and default host policy.
2. Require a caller-supplied `ContextSourceProvider`; the query crate does not
   read files, select runtimes, or depend on a host. Pin every lookup to the
   requested retained generation and validate file size, content hash, UTF-8,
   and span boundaries before indexing text.
3. Bound canonical nodes, source spans, source bytes, postings, and charged
   index payload. Reject missing, stale, malformed, or over-budget input rather
   than publishing a partial index. Do not index whole-file/container spans;
   source-backed facts are candidates, with no language-specific allow-list.
4. Apply source-span augmentation to the current generation only. A historical
   context request keeps the existing source-content label filtering because a
   host's current checkout cannot be assumed to contain historical file bytes.
5. Keep the feature opt-in. Selected-target STEXS evaluations are iteration
   evidence only; default-policy promotion requires whole-repository retrieval
   curves and measured full index-build cost across the approved corpora.

## Consequences

The feature can surface evidence whose query terms occur in code or document
bodies, while preserving default behavior and runtime neutrality. Cold index
construction requires scoped host source access and additional bounded work;
hosts opt in explicitly. Historical queries do not acquire a body-text channel
until a generation-aware source provider exists. The existing serialized
context contract does not change.

## Verification

Tests must prove source-span terms enter exact and stemmed channels, source is
generation-pinned and read through the injected provider, hash/size/span errors
fail closed, and each source/index budget rejects exhaustion without returning
a partial index. Retrieval quality remains a separate whole-corpus acceptance
gate.

## References

- [ADR-0314](0314-bounded-context-retrieval-diagnostic.md)
- [v0 architecture and implementation plan](../V0_ARCHITECTURE_PLAN.md)
