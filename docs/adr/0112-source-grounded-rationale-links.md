# ADR-0112: Extract source-grounded documentation rationale links

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0111 makes authored documentation content searchable as source-backed
`DocumentChunk` facts, and ADR-0110 resolves explicit Markdown links to indexed
source files. That is useful evidence, but a generic `References` edge does not
say whether a linked component is the subject of an ADR decision or an RFC
proposal. The product roadmap (§27) requires a navigable rationale graph while
§28 keeps model-derived semantic facts separate from deterministic extraction.

## Decision

1. Extend the existing Markdown extractor for Markdown files identified as
   ADRs (`adr` path component or `ADR-` filename) and RFCs (`rfc`/`rfcs` path
   component or `RFC-` filename). A link in an ADR's exact `Decision` heading
   section to an explicitly linked supported source file emits the existing
   `Reference` occurrence with external relation namespace
   `syntaxmesh.documentation` and relation `decides`. A link in an RFC's
   `Proposal`, `Proposed API`, or `API Proposal` heading section emits
   `proposes`. Other links retain `References`.
2. These relationships are based only on authored heading structure and
   explicit Markdown destinations. They point to exact indexed `File` nodes,
   not guessed symbols or APIs. Section containment connects the relation's
   source to its verbatim rationale chunks. Do not infer links from prose,
   identifiers, co-change, or model output.
3. Reuse the existing `RelationKind::External`, `Reference`, exact-path
   resolver, provenance, graph generations, and temporal graph storage. Keep
   the reference-occurrence and `ResolvesTo` edges; the resolved external
   relation is an additional source-to-file semantic edge. Missing source
   files remain unresolved with the same relation. No parallel document graph,
   node kind, relation enum variant, DTO, storage table, or database migration
   is introduced.
4. Bump the documentation extractor identity so unchanged Markdown files are
   re-extracted. Keep the stable external namespace and relation names as the
   versioned public labels for this authored relationship slice.
5. Semantic/model-derived claims remain out of scope and must use the separate
   `syntaxmesh-semantic` boundary and provenance policy from the source roadmap.

## Alternatives considered

- Treat every source link in an ADR as a decision: rejected because Context,
  alternatives, and references also contain links.
- Guess code symbols or API members from link fragments: rejected because
  fragment semantics and source-language binding are not established by a file
  link (ADR-0110).
- Add dedicated decision nodes or a documentation-specific edge table:
  rejected for this first slice because existing sections, chunks, references,
  and external relation labels already represent the authored evidence without
  another persistence model.
- Use LLM-generated rationale relationships in the deterministic graph:
  rejected because those claims require distinct provenance, caching, failure,
  and history contracts first.

## Consequences

- Queries can traverse from an ADR Decision or RFC Proposal section through
  source-grounded `decides`/`proposes` edges to exact indexed source files, and
  back through section containment to the authored rationale text.
- Ordinary documentation links and all unresolved occurrences retain their
  existing behavior. Relation identity survives re-resolution when linked
  files are added or removed.
- No wire or database schema version changes are required: the graph already
  serializes `External` namespace/relation strings and indexes source-file
  references using the existing graph path.
- This does not claim member-level API mapping, section-anchor resolution,
  semantic summarization, or general RFC format parsing.

## Verification

- Extractor fixtures cover ADR/RFC role detection, exact heading matching,
  non-semantic links outside those sections, stable IDs, and source spans.
- CLI indexing proves the rationale chunk is searchable, semantic edges reach
  exact indexed source files, ordinary links remain `References`, and missing
  targets retain the `decides`/`proposes` relation as unresolved.
- Re-indexing after adding/removing a linked source file proves relation
  identity and temporal retraction/resolution behavior remain deterministic.

## References

- [Source roadmap §27–28](../syntaxmesh-todo-extensible-opensource-v5.md)
- [ADR-0110](0110-resolve-local-markdown-source-file-links.md)
- [ADR-0111](0111-source-grounded-document-chunks.md)
