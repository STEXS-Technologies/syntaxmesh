# ADR-0111: Index source-grounded documentation chunks

- Status: accepted
- Date: 2026-09-29

## Context

Document and heading nodes make Markdown navigable, but leave authored rationale,
decisions, and procedures unavailable to text search and evidence-backed context.
The initial decision in ADR-0073 deferred paragraph chunks until their identity
and relation semantics were specified. Those semantics are now required for the
project's architecture records and other documentation to participate in the
same graph as source code.

## Decision

1. Add an append-only `NodeKind::DocumentChunk`. Extract Markdown paragraphs,
   code blocks, HTML blocks, tables, and blank-line-delimited UTF-8 text blocks
   as deterministic source facts. Store
   their exact trimmed source text as the canonical node name and their original
   byte range as source evidence; do not summarize, classify, or infer claims.
2. Attach each chunk with `Contains` to its nearest preceding Markdown section,
   or to the document root when outside a section. Plain text chunks attach to
   the document root. Chunk identity derives from its parent, exact text, and
   duplicate ordinal, not line offsets, so inserting unrelated earlier text
   does not churn IDs.
3. Use existing node, edge, provenance, generation-history, and substring-search
   paths. No database schema, transport, DTO, or runtime dependency is added.
   Bump the documentation extractor identity so unchanged documents are
   re-extracted. Append an explicit storage projection code and expose the new
   node kind in analytics export. Bump graph and temporal export schema versions
   because their serialized `Node` payloads now permit this variant.
4. Keep NDJSON as explicit export only. This feature does not use NDJSON for
   storage or inter-service communication.

## Alternatives considered

- LLM/Graphify semantic extraction: rejected for core indexing because it adds
  nondeterministic, unattributed claims and a runtime/service dependency.
- Reuse `Section` for paragraphs: rejected because paragraph evidence and
  heading structure are different graph concepts.
- Keep prose only in the whole-document source: rejected because search and
  context retrieval need bounded, directly attributable evidence units.

## Consequences

Search can retrieve the actual rationale and procedure text authored in Markdown
and plain-text files, and graph traversal links each chunk back to its document
structure. These are source-grounded content units, not a semantic ontology;
cross-document concepts, inferred decision relationships, and summaries remain
future work.

## Verification

Fixtures cover heading ownership, exact source spans, UTF-8, duplicate and
line-shift-stable identities, plain-text paragraphs, search visibility, and
graph context retrieval. Existing serialized variant ordinals remain unchanged.
