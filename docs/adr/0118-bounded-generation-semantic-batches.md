# ADR-0118: Build bounded semantic batches from a pinned generation

- Status: accepted
- Date: 2026-09-29

## Context

Semantic extraction currently requires a caller to build one request per
document file. That prevents provider-neutral implementations from reasoning
over relationships spanning several documents without independently managing
generation reads, section context, and batching. The documentation/rationale
graph roadmap calls for content-addressed semantic enrichment while retaining
the deterministic document graph as the source of truth.

## Decision

1. Add an Engine API to assemble semantic requests from all `DocumentChunk`
   facts in one explicitly selected accepted generation.
2. Read the existing file/node index and generation-pinned `Contains`
   hierarchy; do not rescan, reparse, or add storage. Preserve exact chunk text
   and authored section context, and fail closed on malformed hierarchy.
3. Partition deterministically by file ID and node ID under caller-provided
   maximum chunk-count and UTF-8 byte limits. Count heading bytes as input;
   reject zero limits and any individual chunk that exceeds the byte limit.
   No request may mix graph generations, and no host path or physical node ID
   is added to provider-visible prompt chunks.
4. The Engine only constructs batches. Providers remain host supplied and
   opt-in; durable caching/recovery and publication continue through Penelope
   and ordinary extension fact ingestion. This API does not add an HTTP/local
   provider, token estimation, automatic provider execution, or a CLI/MCP
   surface.

## Consequences

- Provider adapters can extract claims whose exact evidence spans documents
  that fit together in one bounded request.
- Larger corpora are split into independent requests, so cross-batch claims
  are not implied. Callers choose limits appropriate to their provider.
- All semantic outputs retain existing evidence validation, content-addressed
  identities, provenance, retry, and generation publication behavior.
- There is still no built-in provider or automatic documentation enrichment;
  this is a bounded application-layer input path, not a Graphify replacement.

## Verification

- Engine tests cover stable bounded partitioning over a generation containing
  multiple documents, no cross-generation reads, and fail-closed behavior for
  malformed source containment.
- Existing semantic tests continue to verify exact evidence and generation-
  scoped publication.
- The engine fixture now exercises a joint claim with exact quotations from two
  distinct documents, rejects a fabricated quotation without changing source
  facts, retries the prepared request successfully, and reuses its Penelope
  output with a deliberately failing provider. Publication retains both support
  edges and leaves the historical source snapshot unchanged. This is mocked
  cross-document plumbing, not evidence of real-model synthesis quality.

## References

- [SyntaxMesh §27: Documentation / rationale / ADR graph](../syntaxmesh-todo-extensible-opensource-v5.md)
- [ADR-0115: Source-grounded document semantic facts](0115-source-grounded-document-semantic-facts.md)
- [ADR-0117: Include authored section context](0117-document-section-context-for-semantic-input.md)
