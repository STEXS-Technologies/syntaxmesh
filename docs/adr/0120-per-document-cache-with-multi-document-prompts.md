# ADR-0120: Cache semantic document requests individually and batch model prompts

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0119 made semantic indexing a single opt-in CLI command and bounded
provider calls, but its request cache key covers the entire multi-document
batch. An edit to one document can therefore invalidate the cached output for
unchanged documents that shared that request. Graphify's documented flow
retains semantic cache entries per file while grouping roughly 20–25 uncached
documents from the same directory for parallel extraction. SyntaxMesh already
has the right durable boundary: Penelope prepares and caches independent
`SemanticRequest`s, then passes only misses to `SemanticProvider::extract_many`.

## Decision

1. Add an Engine method that creates deterministic, chunk/byte-bounded
   `SemanticRequest`s per document. A document larger than the configured
   bounds is partitioned into stable bounded requests within that document;
   requests never cross file ownership boundaries. Preserve the existing
   generation-wide batching API for callers that explicitly want cross-file
   semantic requests.
2. The CLI provider's `extract_many` packs pending document requests into
   deterministic multi-document prompts. A prompt is bounded by 22 requests,
   32 chunks, and 48 KiB of chunk text plus authored headings, with at most
   four HTTP calls in flight. An individually oversized request is returned
   as an error, never silently truncated.
3. Validate the model output against the complete packed prompt before
   distributing claims back to each document request by the cited chunk
   content hashes. Every claim's evidence must belong to exactly one
   independent cache key. A claim that combines evidence from multiple cached
   requests rejects the packed group rather than producing cache entries that
   could become stale independently. The prompt may use neighboring documents
   to disambiguate terminology, but each accepted claim must be supported
   within its own document request. Identical independently supported claims
   are merged with all their source evidence before publication. Unknown
   hashes, invalid quotes, or malformed relations fail the packed group.
4. Penelope persists each document-scoped result independently. On later runs,
   unchanged documents remain cache hits when a neighboring document changes;
   only missing/changed requests are packed and sent to the model. The merged
   semantic fact layer is still replaced atomically in one graph generation,
   retaining prior history.
5. Directory-local grouping is a prompt-packing preference, not a cache
   identity or correctness condition. Initial implementation may use stable
   generation/file ordering; directory grouping can be added without changing
   the cache or provider contract.

## Consequences

- A single changed document no longer invalidates all neighboring document
  extraction records solely because they shared a model prompt.
- First-run model calls still see multiple documents together, reducing
  request fanout and allowing context to disambiguate terminology. Claims
  whose truth requires combining facts across independently cached documents
  are deliberately rejected; identical claims independently supported by
  several documents merge into one graph assertion with multiple evidence
  records.
- A failure in one packed model call leaves each constituent Penelope job
  prepared and retryable; successful independent calls remain reusable.
- This reduces invalidation scope, but a large document split across multiple
  bounded requests can still cause neighboring chunks within that document
  to be reprocessed if the document's chunk ordering/partition changes.
- Model quality, latency, and token costs still require evaluation against a
  real local model and representative document corpora.

## Verification

- Engine tests prove requests are bounded and never mix owner files.
- CLI loopback tests prove multiple documents share one provider call, claims
  are correctly split and merged, an edited document reuses an unchanged
  document's cached output, and an unchanged repeat makes no provider request
  or graph generation.
- Penelope tests prove separate per-document durable records are completed and
  reused on an identical second request; existing restart/retry fixtures cover
  a prepared single semantic request. A partial-failure regression proves a
  completed document cache hit survives a peer failure and the retry submits
  only the still-prepared request.
- Existing exact evidence validation, engine replacement/history, architecture
  boundary, and cross-backend tests remain required.

## References

- [Graphify semantic extraction workflow](https://github.com/Graphify-Labs/graphify/blob/v8/graphify/skill-trae.md)
- [ADR-0115: Source-grounded document semantic facts](0115-source-grounded-document-semantic-facts.md)
- [ADR-0116: Penelope semantic enrichment cache](0116-penelope-semantic-enrichment-cache.md)
- [ADR-0118: Bounded generation semantic batches](0118-bounded-generation-semantic-batches.md)
- [ADR-0119: One-command semantic indexing](0119-one-command-parallel-semantic-indexing.md)
