# ADR-0117: Include authored section context in semantic document inputs

- Status: accepted
- Date: 2026-09-29

## Context

The documentation extractor emits exact `DocumentChunk` text and a
parent-to-child `Contains` hierarchy from each `Document` through nested
`Section` nodes. The current semantic request contains only chunk text. A
paragraph such as “this is required” loses its subject if the authored heading
is omitted, and identical text under different headings could incorrectly
share a provider result/cache entry.

## Decision

1. Each `SemanticDocumentChunk` carries an ordered section-context path, from
   outermost authored section to nearest section. It contains section names
   only—not physical file paths or graph IDs.
2. Providers receive the section path as context alongside exact chunk text.
   Evidence quotes must still be verbatim substrings of the chunk text; a
   heading may inform meaning but is not accepted as evidence for a claim.
3. Content identity for a semantic chunk includes exact text and ordered
   section context. The request/cache fingerprint includes every such identity
   and still excludes physical file/node IDs, so equal authored content and
   context can be reused across snapshots/worktrees.
4. Engine request construction walks the generation-pinned `Contains`
   hierarchy from a chunk to its enclosing sections and uses the existing
   per-file node index. It does not reparse documents or add a parallel content
   store. If the graph has malformed/missing hierarchy nodes, request creation
   fails instead of silently dropping context.

## Consequences

- Provider input retains authored Markdown/plain-text structure needed to
  interpret architectural decisions and rationale.
- Editing a section heading invalidates semantic cache entries for chunks
  under that section, even when paragraph bytes are unchanged.
- The graph's section facts remain the authored source of context; inferred
  claims retain exact quoted chunk evidence and `SemanticInference` provenance.
- Host integrations must include context in prompt construction; they must not
  promote contextual headings into source quotations.

## Verification

- The Engine returns the expected outer-to-inner section path from a pinned
  generation and does not expose repository paths in prompt chunks.
- Same text under different section context receives a different semantic
  cache key; same text and context under different physical paths/IDs reuses
  the key.
- Exact-quote validation continues to accept only chunk-body substrings.
- An invalid/missing `Contains` parent fails closed.

## References

- [SyntaxMesh §27: Documentation / rationale / ADR graph](../syntaxmesh-todo-extensible-opensource-v5.md)
- [ADR-0111: Source-grounded document chunks](0111-source-grounded-document-chunks.md)
- [ADR-0115: Source-grounded document semantic facts](0115-source-grounded-document-semantic-facts.md)
- [ADR-0116: Penelope semantic enrichment cache](0116-penelope-semantic-enrichment-cache.md)
