# ADR-0074: Resolve local Markdown links to indexed documents

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0073 adds typed document roots and Markdown heading sections, but currently
discards links between documentation files. The product roadmap calls for a
documentation/rationale graph and explicitly models documents as references to
other engineering artifacts. SyntaxMesh already has a stable source-reference
contract and a conservative unique-name resolver that retains unresolved
occurrences. Reusing it keeps link evidence in the same graph and temporal
history rather than creating a special document-only graph.

## Decision

1. Extract only local Markdown links whose destination resolves lexically to a
   repository-relative `.md`, `.markdown`, `.txt`, `.text`, `.rst`, `.adoc`, or
   `.asciidoc` path. Ignore network,
   protocol-relative, mail, and fragment-only destinations. Ignore URL query
   and fragment components for target-document selection; section-anchor
   resolution is deferred.
2. Emit existing SDK `Reference` occurrences with `RelationKind::References`,
   anchored to the containing heading section (or document root outside a
   heading). Normalize `.`/`..` components without allowing a path to escape
   the repository. Preserve the original link source range and producer
   provenance. Stable occurrence IDs depend on source node, normalized target,
   and same-target occurrence ordinal, not line numbers.
3. Make `NodeKind::Document` eligible for the existing conservative symbol
   resolver. The document node name is its normalized repository-relative
   path, so exact normalized local link targets resolve uniquely. Ambiguous,
   unavailable, malformed, or out-of-index targets remain normal explicit
   unresolved/ambiguous reference facts; they are never fetched from a network.
4. Do not resolve `#anchor` to a heading yet. CommonMark/GitHub slug rules,
   explicit heading IDs, duplicate headings, and external processor behavior
   need a separate contract. A path-plus-fragment link still identifies its
   containing document only.

## Alternatives considered

- Create a second document-link graph and persistence table: rejected because
  `Reference`, the existing resolver, and temporal fact storage already model
  source-backed relationship occurrences.
- Resolve URL/network links or interpret arbitrary URL schemes: rejected as
  non-deterministic, unsafe, and outside local repository indexing.
- Guess section anchors from heading text: deferred because slug conventions
  vary and incorrect links would create false certainty.
- Add a document-specific provider mode: deferred until anchor or cross-format
  link semantics require more than unique exact path matching.

## Consequences

- Local document-to-document links become generation-scoped `References`,
  `ResolvesTo`, and semantic `References` edges when the target is indexed.
- Existing unresolved-reference diagnostics expose missing local documents
  without losing the authored link occurrence.
- No new fact kinds, serialized fields, storage tables, or migration are
  introduced. At implementation time graph schema v6 and temporal schema v8
  covered the Node and Edge payloads. Temporal NDJSON has since advanced to v9
  for generation-pinned historical-neighbor records (ADR-0086).
- Code-file links and heading-anchor links remain unresolved or ignored,
  respectively; this is intentionally an initial local-document-link slice.

## Verification

- Unit fixtures cover relative/root-relative links, query/fragment stripping,
  external-scheme exclusion, traversal rejection, UTF-8 spans, and stable IDs
  across line shifts.
- Mixed CLI indexing proves linked documents resolve only when both documents
  are indexed; missing targets stay explicit and source occurrences persist.
- File, SQLite, and Turso backend conformance/restart/history tests retain the
  generated reference and resolution facts without additional migrations.
- The strict workspace CI suite passes without new lint allowances.
