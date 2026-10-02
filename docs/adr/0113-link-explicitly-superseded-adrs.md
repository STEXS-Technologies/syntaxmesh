# ADR-0113: Link explicitly superseded ADRs

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0112 adds source-grounded `decides` and `proposes` relations for explicit
links inside ADR/RFC sections. ADR history also contains explicit replacement
metadata: for example, ADR-0065's status line links to ADR-0066 as its
successor. Treating this as an ordinary `References` edge hides a useful
decision-history relationship; inferring it from arbitrary mentions would be
unsafe.

## Decision

1. In an ADR Markdown file, recognize a local Markdown link on the ADR's
   explicit `- Status: superseded by ...` metadata line, and only when its
   target is an ADR Markdown path. Emit the existing `Reference` occurrence
   with external relation namespace `syntaxmesh.documentation` and relation
   `superseded_by`, sourced from the ADR document root. The edge direction is
   the old decision to its explicitly linked successor.
2. Continue to treat all other links—including bare ADR number mentions,
   ordinary Context/References links, and status text without a linked ADR—as
   normal `References` or unresolved references. Do not interpret `replaced`,
   `supersedes`, or arbitrary prose as this relation in this increment.
3. Reuse the `External` relation, existing `Reference` resolution, provenance,
   and temporal graph path. Keep the occurrence and `ResolvesTo` edges; when
   the target ADR is indexed, add the direct `superseded_by` edge. Preserve the
   relation on unresolved targets. No new node, DTO, storage schema, migration,
   or export format is added.
4. Bump the documentation extractor identity. Add the relation to the
   documentation graph roadmap; future status spellings require explicit
   fixtures and an ADR rather than heuristic expansion.

## Alternatives considered

- Infer supersession from arbitrary prose or any link in a `Superseded` section:
  rejected because a document may discuss another decision without replacing
  it.
- Emit the inverse `supersedes` relation from the linked successor: rejected
  because the status metadata is authored on the old ADR and target resolution
  should preserve source direction without synthesizing a reverse occurrence.
- Add a dedicated supersession node/table or relation enum variant: rejected
  because the existing extensible `External` relation already preserves the
  namespaced semantic edge through storage and exports.

## Consequences

- Explicitly superseded ADRs form a source-grounded, generation-scoped decision
  history edge from the old ADR to the successor.
- Missing successor ADRs remain unresolved with the same `superseded_by`
  relation and can resolve later through the existing exact-path mechanism.
- This does not infer the full ADR supersession graph from non-link text or
  support alternate metadata spellings.

## Verification

- Extractor tests prove only the ADR status-line link receives
  `superseded_by`, the occurrence originates at the document root, and ordinary
  ADR links remain `References`.
- CLI indexing proves ADR-0065-style links resolve to the exact target document
  and missing linked ADRs remain unresolved with their relation retained.

## References

- [Source roadmap §27](../syntaxmesh-todo-extensible-opensource-v5.md)
- [ADR-0112](0112-source-grounded-rationale-links.md)
- [ADR-0065](0065-typed-import-export-source-facts.md)
