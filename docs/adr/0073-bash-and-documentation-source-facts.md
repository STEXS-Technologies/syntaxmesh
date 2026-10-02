# ADR-0073: Extract Bash and Markdown source facts in Rust

- Status: accepted
- Date: 2026-09-28

## Context

The product scope includes Rust, TypeScript/JavaScript, Python, Bash scripts,
and engineering documentation such as Markdown and plain text. Current scanning
and extraction cover only the first four programming languages except Bash;
documentation ingestion is not implemented. The roadmap calls for a Markdown/
ADR parser and requires a clear typed graph model before adding a source format.

Parsing must remain deterministic and runtime agnostic. Indexing may not launch
Python, Node, Bash, or another host runtime to understand source. New durable
facts must preserve existing enum ordinals, stable identities, source evidence,
and append-only history behavior.

## Decision

1. Add dedicated Rust language packs for Bash and documentation, composed by
   the existing `CompositeExtractor` and ignore-aware scanner. Use the direct
   crates.io Tree-sitter Bash grammar for syntax structure and `pulldown-cmark`
   for CommonMark parsing. Neither parser executes the language/document.
2. Add append-only `NodeKind` variants `Script`, `Document`, and `Section`, plus
   `RelationKind::Contains`. A script/document is the source file's typed root;
   Bash function declarations are `Function` nodes and section headings are
   `Section` nodes. `Contains` links those declarations/sections to their
   containing script/document. Every fact has normal file ownership,
   deterministic ID, provenance, and byte-range evidence.
3. Bash extraction records function definitions and conservative command-call
   references from executable command positions. It does not execute or source
   scripts, expand shell variables, follow dynamic `eval`, interpret shell
   configuration, or claim complete command resolution. Unsupported syntax
   remains an extraction diagnostic/error rather than a guessed fact.
4. Markdown extraction records document roots and heading sections, preserving
   exact source ranges. Plain-text `.txt`, `.text`, `.rst`, `.adoc`, and
   `.asciidoc` files are represented as one document root. These non-Markdown
   formats are UTF-8 text inputs, not format-aware parsers. Paragraph-level
   semantic chunking is deferred until its identity and relation semantics are
   specified. Frontmatter semantics, embedded-language extraction, and inferred
   document-to-code relations are not part of this slice.
5. Append new serialized enum variants only; preserve all existing bincode
   ordinals. Assign explicit unused SQL node-kind projection codes. No schema
   migration is required because the projection is an integer column and new
   rows are maintained/validated through the existing canonical payload path;
   no indexed query is added in this slice. Bump graph export schema 5→6 and
   temporal export schema 7→8 because those records carry the serialized
   `Node` payloads and can now contain the added variants.
6. Add `.sh`, `.bash`, `.md`, `.markdown`, `.txt`, `.text`, `.rst`, `.adoc`, and
   `.asciidoc` to the configured CLI extractor/scanner set. Input bytes must be
   valid UTF-8 as for current source scanning; no alternate runtime is
   introduced.

## Alternatives considered

- Execute Bash/Python/Node or invoke documentation tooling during extraction:
  rejected because it is nondeterministic, unsafe for untrusted repositories,
  and violates the Rust-native runtime boundary.
- Encode scripts/documents/sections as `External` or overload `Module` and
  `Function`: rejected because these are first-party typed source facts and
  those labels would be semantically misleading.
- Add separate scanner paths or storage tables: rejected because existing
  extension routing, provenance, temporal fact storage, and graph queries
  already provide the required lifecycle.
- Parse all document prose into semantic chunks now: deferred; stable chunk
  identity and query/value semantics need a separate evidence-backed design.

## Consequences

- Bash and Markdown/text inputs become discoverable and source-backed in the
  same generation/history model as existing languages.
- Bash facts are conservative syntax facts, not shell execution semantics.
- Markdown headings provide structural navigation; arbitrary prose remains
  text attached to the document rather than fabricated ontology.
- Consumers that deserialize new fact kinds need an updated SyntaxMesh build;
  older binaries cannot read newly emitted variants, consistent with other
  additive fact variants.

## Verification

- Golden extraction fixtures cover Bash functions/calls, comments/strings,
  command substitutions, syntax errors, and stable IDs across line shifts.
- Markdown fixtures cover ATX/setext headings, nested sections, duplicate
  headings, inline code exclusion, and UTF-8 byte spans; `.txt` has one root.
- Composite routing, scanner inclusion/ignore behavior, and mixed-language
  CLI indexing prove files reach their Rust extractors.
- Durable SQLite/Turso and File history fixtures prove new node/edge facts
  survive restart and historical `GraphAt` without migration or fact loss.
- Strict Clippy, workspace tests, DTO/schema compatibility checks, and
  architecture boundary checks pass with no lint allowances added.
