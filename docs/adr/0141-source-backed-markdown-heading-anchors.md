# ADR-0141: Source-backed Markdown heading anchors

- Status: accepted
- Date: 2026-09-30

Use the crates.io Rust `github-slugger` 0.1.0 profile for heading slugs and
document-wide collision suffixes. Sim already uses `rehype-slug` for authored
Markdown heading navigation; reuse that approach without its JavaScript runtime
or a hand-written slug algorithm. Parsing remains in the existing pulldown-cmark
documentation pack. Include the slugger version in extractor identity so old
documents re-extract through the existing selective invalidation mechanism.

Represent a nonempty heading anchor as a source-backed `NodeKind::External`
with namespace `syntaxmesh.source`, kind `anchor`, and exact qualified name
`normalized/file.md#slug`. Its stable ID derives from file identity and qualified
name; source/provenance point at the authored heading. A `Contains` edge attaches
it to the existing, unchanged-title `Section`. This reserves a generic SDK
source-anchor convention, not a new core enum, alias table, or parser dependency
in core. SDK target classification is shared by indexer and resolver. Existing
exact-name node indexes serve persistence and incremental rebinding.

Retain Markdown fragments, including same-document references. Decode percent-
encoded fragment text using crates.io `percent-encoding`; invalid UTF-8/control
fragments are not accepted. Unknown/case-mismatched fragments remain unresolved
instead of falling back to the document root. Source-code and non-Markdown links
keep their existing file-level behavior; explicit HTML/custom heading IDs, path
percent-decoding, and browser-wide Markdown compatibility remain out of scope.

Resolve source-anchor names byte-exactly before symbol normalization; never add
terminal symbol aliases for anchor definitions. Existing source-occurrence,
`ResolvesTo`, provenance, and history formats remain unchanged. Test duplicate
headings, formatted/Unicode titles, local/external/missing fragments, rebinding
after heading edits, and retained historical edges. No AI/provider is required.

Upstream: <https://crates.io/crates/github-slugger>.
