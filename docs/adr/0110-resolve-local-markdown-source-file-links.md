# ADR-0110: Resolve local Markdown links to indexed source files

- Status: accepted
- Date: 2026-09-29

## Context

[ADR-0074](0074-resolve-local-markdown-document-links.md) uses the existing
source-backed `Reference` contract for local document links and explicitly
leaves links to code files unresolved or ignored. Source and document inputs
are now both indexed by the Rust-hosted composite extractor, so a local link
from project documentation to an indexed source file should participate in
the same evidence-backed graph instead of being silently dropped.

## Decision

1. Extend the existing Markdown link extractor to recognize only the source
   and documentation extensions selected by the default scanner: Rust,
   TypeScript/JavaScript, Python, Bash, and documentation text.
2. Continue to normalize repository-relative paths and reject traversal beyond
   the repository root. Strip query and fragment components for target-file
   selection. Do not fetch network links, interpret URL schemes, or resolve
   fragment anchors to symbols or sections.
3. Resolve an exact normalized path only when it names a `File` node in the
   indexed generation. Reuse `Reference`, `RelationKind::References`, the
   existing unique-path resolver, provenance, and temporal graph storage.
   Missing or unindexed targets remain explicit unresolved reference facts.
4. Do not infer member-level links from a fragment, parse source files from
   links, or introduce cross-language symbol resolution.
5. Bump the documentation extractor identity so incremental indexing
   re-extracts prior Markdown files and records newly recognized links.

## Alternatives considered

- Add a Markdown-specific storage table or edge DTO: rejected because existing
  reference occurrences and temporal graph facts already preserve source span,
  provenance, and unresolved targets.
- Resolve links to functions/classes named after fragments: rejected because
  Markdown anchor conventions vary and file links do not establish a source
  language's symbol semantics.
- Accept arbitrary local extensions: rejected because only files selected by
  the configured in-scope scanner can be targets in the default graph.

## Consequences

- A Markdown link to an indexed supported source file resolves to its exact
  `File` node and creates the ordinary `References`/`ResolvesTo` facts.
- Links to missing, ignored, or unsupported files remain unresolved or are
  ignored according to the existing lexical safety rules.
- No public DTO, canonical node/edge encoding, database schema, or migration
  changes. Other hosts using a custom scan scope may index only a subset of
  these supported extensions; target resolution remains generation-bound.
- Heading anchors, member-level source links, and cross-language binding stay
  out of scope.

## Verification

- Extractor tests cover supported extension filtering, path normalization,
  query/fragment stripping, traversal rejection, and stable occurrence IDs.
- Resolver tests prove exact file-path matching without terminal-name
  guessing.
- A Rust CLI fixture checks an indexed source-file link resolves, a missing
  source target stays explicit, and an external URL creates no local link.
