# ADR-0077: Represent Rust `use` trees as typed import source facts

- Status: accepted
- Date: 2026-09-29

## Context

The Rust pack extracts declarations and call references, but it drops Rust
`use` declarations. SyntaxMesh already has a language-neutral typed import
event, canonical `NodeKind::Import` lowering, source provenance, temporal
history, and query support. Adding a Rust-specific fact table or resolver path
would duplicate infrastructure that is already used by ECMAScript and Python.

Rust `use` syntax is not a module specifier in the Node or Python sense. It can
bind modules, types, values, traits, macros, or aliases; grouped use trees
expand to multiple bindings; glob imports do not enumerate names; and `pub use`
also carries re-export visibility. Source extraction must preserve what syntax
actually says without claiming a module target or symbol has been resolved.

## Decision

1. Append `RustUse` and `RustGlob` to `ImportKind`. Existing bincode enum
ordinals must not move. A `RustUse` is one named binding leaf in a Rust use
tree; a `RustGlob` is one `::*` leaf.
2. Flatten nested groups into one `ImportRecord` per leaf. `specifier` stores
the complete target path reconstructed from the AST (including `crate`,
`self`, `super`, or a leading absolute path where present). `imported_name`
stores the leaf name for a named binding when one exists; `local_name` stores
the effective local binding, including `_` for an explicit `as _`. For
`RustGlob`, both names are absent and `specifier` is the path before `::*`.
These Rust fields describe import paths, not proven module specifiers.
3. Preserve a source span covering the corresponding leaf syntax, source file
and content hash, stable occurrence identity, and extractor provenance.
Identity depends on module identity and semantic shape plus deterministic
duplicate disambiguation, not byte offsets or line numbers.
4. Emit one file-module node for every Rust source, matching the existing
ECMAScript/Python convention, and attach each typed import occurrence to that
module through existing `RelationKind::Imports` lowering. Do not add tables,
fact families, DTO fields, or storage migrations.
5. Do not interpret `pub use` as an export in this slice: `ImportRecord` has no
visibility field, and creating an export without preserving visibility would
be misleading. Public re-export modeling, `extern crate`, macro expansion,
and symbol/module resolution remain separate work.
6. Existing Node and Python resolution providers must not receive Rust use
occurrences. Rust facts remain explicit source occurrences with no resolution
edge or diagnostic until a Rust-aware resolver contract is designed.
7. Bump the Rust extractor semantic version so existing unchanged Rust files
are re-extracted by the normal producer-identity invalidation mechanism.

## Alternatives considered

- **Reuse `Reference` and the call resolver:** rejected; a Rust import path is
  not a call and terminal symbol-name matching would invent relationships.
- **Use ECMAScript `Named` or Python import kinds:** rejected; they misstate
  the syntax and binding semantics.
- **Add a separate Rust import table/history family:** rejected; the existing
  typed import node already supplies identity, provenance, persistence,
  generation history, and queries.
- **Resolve Rust paths during extraction:** rejected; extraction is
  file-local, while Rust module layout, crate identity, aliases, and Cargo
  configuration require a repository-aware stage.

## Consequences

- Rust imports become source-backed, typed, queryable, and historical using
  the existing graph model.
- Import source facts from all supported code languages share graph lowering,
  without treating their distinct path semantics as interchangeable.
- Rust `pub use` visibility and re-export semantics remain unavailable until
  the model can preserve them explicitly.

## Verification

- Cover simple, renamed, grouped, nested, glob, `crate`/`self`/`super`, and
  absolute use paths, plus explicit `as _` and duplicate occurrences.
- Verify exact source evidence, stable IDs under line movement, and extractor
  identity invalidation.
- Verify typed node/module ownership lowering and history/restart through
  existing backend conformance, without adding a migration.
- Verify Node and Python providers produce neither resolution edges nor
  diagnostics for Rust import occurrences.
