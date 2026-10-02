# ADR-0072: Add an explicit static Python module-resolution profile

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0071 records Python import syntax without resolving it. The current Oxc
provider and Node CLI profile implement ECMAScript resolution and must not be
reused to interpret Python paths. No sibling project in the workspace provides
a reusable Python source resolver. SyntaxMesh already has the useful boundary:
language-specific providers implement the runtime-neutral
`ModuleResolutionProvider`, the Indexer records outcomes as temporal facts,
and the CLI injects filesystem access. Follow that pattern rather than
introducing parser-side lookup or another graph/storage path.

Python imports are affected by executable `sys.meta_path` hooks, zip imports,
installed distributions, namespace packages, and launch-context-dependent
`sys.path`. A static local profile cannot soundly emulate arbitrary Python
execution. The first profile therefore needs a precise, conservative supported
subset and must keep unsupported cases explicit.

## Decision

1. Append `PythonModule`, `PythonFrom`, and `PythonStar` modes to the existing
   runtime-neutral `ModuleResolutionMode`. Preserve existing variants and
   bincode/serialized ordinals.
2. Add a default `supports(request)` capability to `ModuleResolutionProvider`.
   The Indexer invokes only providers that support an occurrence; unsupported
   source kinds remain source facts without false unresolved diagnostics.
   Compose multiple providers in configured order through one provider with a
   stable aggregate identity. Oxc supports ECMAScript files and import/CommonJS
   modes only; the Python provider supports `.py` files and Python modes only.
3. Implement Python lookup in `syntaxmesh-lang-python` as an injected
   `ModuleResolutionProvider`, using a small filesystem port rather than
   ambient filesystem access. It resolves source modules only; it does not
   execute Python, inspect installed packages, or bind imported members.
4. Support configured repository-relative source roots in order. Within a
   root, resolve classic packages (`directory/__init__.py`) before sibling
   modules (`module.py`), requiring classic package initializers for package
   components. Resolve relative specifiers from the source file's package
   context. A returned path is normalized repository-relative and remains
   subject to the Indexer's indexed-module inventory check.
5. Do not claim support for namespace packages, zip imports, `sys.path` outside
   configured roots, import hooks, installed distributions, runtime-dependent
   module attributes, or member-level binding. Return deterministic unresolved
   outcomes for lookup misses; invalid relative levels are invalid outcomes.
6. Extend project configuration with an ordered `profiles` list while
   preserving the existing singular `profile = "node"` form. Allow Node and
   Python profiles together so mixed-language projects use independent
   providers. Add a Python init option and optional source roots; omitted roots
   default to the repository root.

## Alternatives considered

- Run a Python interpreter or `importlib` during indexing: rejected because it
  executes user code, depends on the active environment, and violates the
  deterministic/runtime-agnostic provider boundary.
- Route Python imports through Oxc: rejected because Node resolution rules do
  not implement Python package and relative-import semantics.
- Resolve imports in the Tree-sitter extractor: rejected because extractors
  are file-local and resolution depends on repository inventory and profile
  configuration.
- Add provider selection logic and separate query/storage tables to the
  Indexer: rejected because the capability method and existing typed temporal
  diagnostics already express this lifecycle.

## Consequences

- Python module edges and typed diagnostics become available when the explicit
  Python profile is enabled; extraction remains independent of resolution.
- The resolver is intentionally narrower than Python's runtime import system.
  Unsupported or ambiguous runtime behavior is not guessed.
- Existing single-profile Node configuration remains valid. No database
  migration is needed: new modes are nested values in existing versioned
  `Import` nodes and diagnostics use their existing graph fact representation.

## Verification

- Cover absolute and relative imports, package `__init__.py`, module files,
  source-root ordering, parent traversal rejection, and unsupported cases
  against an injected filesystem.
- Prove providers do not receive unsupported language/mode combinations, and
  mixed Node+Python configuration resolves each language only with its own
  provider.
- Verify indexed-target filtering, typed diagnostic provenance, changed
  resolver settings, restart, and historical graph reads using existing tests.
- Preserve the current Node profile behavior and backward-compatible config.
