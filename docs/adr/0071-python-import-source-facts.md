# ADR-0071: Represent Python imports as typed source facts

- Status: accepted
- Date: 2026-09-28

## Context

The Python extractor emits declarations and call references, while SyntaxMesh's
existing `ImportRecord` contract and graph lowering are currently produced by
the ECMAScript pack. Gate 5 requires a Python call/import graph. Reusing the
existing occurrence identity, provenance, graph lowering, temporal history,
and query machinery is preferable to adding a Python-only storage family.

Python import syntax is not interchangeable with ECMAScript import syntax.
`import package.module as alias`, `from .module import name as alias`, and
`from package import *` need distinct source classifications. Treating them as
JavaScript default/named/namespace imports would encode misleading semantics.
The existing opt-in Oxc resolver is Node-specific and must not resolve Python
occurrences or emit Node-resolution diagnostics for them.

## Decision

1. Extend the existing `ImportKind` with appended `PythonModule`, `PythonFrom`,
   and `PythonStar` variants. Appending preserves serialized enum ordinals.
2. Emit one `ImportRecord` per imported binding. `PythonModule` stores the full
   dotted module specifier and the binding name (alias, or the first component
   for an unaliased dotted import) as `local_name`. `PythonFrom` stores the
   exact module specifier, imported member, and effective local binding.
   `PythonStar` stores the module specifier with no imported/local name.
   Relative prefixes remain in the source specifier (for example `.helpers`);
   this is source evidence, not a normalized resolution path.
3. Preserve each binding's exact source span, extractor provenance, and stable
   identity based on module identity plus semantic shape and deterministic
   duplicate disambiguation, never byte offsets. Future imports are not emitted
   as ordinary module dependencies in this slice.
4. Lower records through the existing typed `NodeKind::Import` graph fact and
   existing graph/history/query contracts. No new store tables or migrations
   are introduced. Bump the Python extractor semantic version so unchanged
   files are re-extracted through the existing identity invalidation path.
5. Do not pass Python-specific import kinds to the Oxc Node resolver. They
   remain explicit source facts without resolution edges or resolution
   diagnostics until a Python-aware resolution profile is designed and
   accepted separately.

## Alternatives considered

- Reuse ECMAScript `Default`, `Named`, or `Namespace` kinds: rejected because
  their semantics do not accurately describe Python module/member/star forms.
- Add Python-specific persisted tables or a separate query path: rejected
  because imports already participate in typed node history and graph queries.
- Resolve Python paths in the parser or through the Node resolver: rejected
  because extraction is file-local and deterministic, while module resolution
  depends on repository/package configuration and language-specific rules.

## Consequences

- Python imports become queryable, provenance-backed temporal source facts
  using the current graph contract.
- Python module resolution, `__init__.py` package rules, namespace packages,
  import hooks, and symbol/member binding remain explicitly unsupported.
- The public serialized `ImportKind` enum gains variants; existing variants
  and their ordinal positions remain unchanged.

## Verification

- Cover module imports, aliases, dotted imports, from-imports, relative
  imports, star imports, and multiple bindings with exact source spans.
- Prove stable IDs under line movement and distinct duplicate occurrences.
- Verify indexer lowering, add/update/remove history, restart, `GraphAt`, and
  cross-backend conformance through existing import graph tests.
- With the Node profile enabled, Python import occurrences remain un-resolved
  source facts and do not produce Node resolver calls or diagnostics.
