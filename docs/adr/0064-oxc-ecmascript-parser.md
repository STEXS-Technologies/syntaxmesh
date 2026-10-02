# ADR 0064: Use Oxc for ECMAScript parsing

- Status: accepted
- Date: 2026-09-28

## Context

The TypeScript and JavaScript language packs currently use Tree-sitter. A
validation run over the sibling Sim project (2,501 `.ts`, `.tsx`, `.js`, and
`.jsx` files) found that the pinned TypeScript grammar rejects valid modern
syntax, including `importOriginal<typeof import('@/lib/utils')>()`. A local
same-width JSX ampersand recovery was added, but parser-specific recovery is
fragile and still cannot address broader grammar lag. The unofficial
`brokk-tree-sitter-typescript` fork was also tested and rejected the corpus
expression.

The official Oxc parser is available directly from crates.io, supports
JavaScript, TypeScript, JSX, and TSX, and is compatible with the workspace MSRV.
It is a parser library, not a JavaScript runtime. Replacing the parser does not
change the language extractor, fact DTOs, provenance model, or host/runtime
boundaries.

## Decision

- Use Oxc 0.152.0 for parsing JavaScript and TypeScript in
  `syntaxmesh-lang-ecmascript`; depend on crates.io packages directly.
- Select Oxc `SourceType` from the validated source-file extension and reject
  fatal parse errors and recoverable syntax diagnostics. Do not publish partial
  facts from malformed input.
- Build existing SyntaxMesh declaration and call-reference facts from the Oxc
  AST, retaining stable node identity rules, source byte spans, file-bound
  provenance, and conservative resolution behavior.
- Emit one source-backed `Module` node per ECMAScript file and represent static
  imports, re-exports, literal dynamic imports, and literal CommonJS
  `require()` calls as raw `Imports` references from that module. Do not resolve
  module specifiers in the extractor.
- Keep parser dependencies private to the ECMAScript language pack. Python
  remains on Tree-sitter. No runtime, transport, host, database, or workflow
  dependency is introduced.
- Bump ECMAScript extractor producer identities when the parser/extraction
  implementation changes so unchanged files are re-extracted.

## Alternatives considered

- Keep Tree-sitter and wait for upstream grammar releases: rejected because
  valid supported-project source is already blocked, with no dependable release
  timeline.
- Adopt the tested unofficial Tree-sitter fork: rejected because it did not
  parse the failing construct and would weaken dependency provenance.
- Use the TypeScript compiler or Node.js as a parser: rejected because it adds
  a host runtime dependency and complicates runtime-agnostic embedding.
- Recover from parser errors and publish partial facts: rejected because the
  current extraction contract has no completeness diagnostic channel and
  silently incomplete graphs are unsafe.

## Consequences

- The ECMAScript pack gains Oxc's Rust dependencies and an allocator per parse;
  these remain isolated from core and public crates.
- The initial Oxc visitor maps classes, functions, methods, directly-bound arrow
  functions, calls, constructor calls, and source module dependencies to
  existing SyntaxMesh facts. It does not extract import binding/alias semantics
  or claim complete ECMAScript semantic extraction.
- Full Sim indexing verifies parser/extractor/indexer integration on one
  realistic project, not completeness across ECMAScript syntax or projects.

## Verification

- ECMAScript pack tests pass for declarations/references, original source
  spans, module nodes, static imports, re-exports, literal dynamic imports,
  literal `require()` calls, malformed input, JSX ampersands, and modern
  TypeScript import types.
- The `parse_corpus` example parsed all 2,550 supported ECMAScript files in the
  sibling Sim tree with zero diagnostics.
- SyntaxMesh CLI indexed the full 2,557-file supported-language Sim tree into a
  durable File generation (72,447 nodes and 74,750 edges, including 11,977
  unresolved import occurrences). Reopened status reported current freshness,
  no unindexed/changed/removed files, a matching canonical root, valid
  references, and healthy logical integrity.
- `cargo make ci` passes with the Oxc production extractor. Existing allowed
  dependency audit/license warnings are unrelated to the parser migration. The
  corpus result and remaining semantic limitations are recorded in the v0
  architecture plan.
