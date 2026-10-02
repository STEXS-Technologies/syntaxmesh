# ADR-0067: Resolve ECMAScript modules through an injected provider

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0066 preserves typed import and export occurrences and deliberately leaves
their source specifiers unresolved. The next required step is resolving these
occurrences against the repository's actual module rules: source extensions,
directory indexes, package `exports`/`imports`, TypeScript path mappings, and
the selected ESM/CommonJS conditions. A small hand-written candidate list
would give false confidence and diverge across NodeNext, Bundler, and package
configurations.

The Oxc project already maintains `oxc_resolver`, a Rust implementation of
Node ESM/CommonJS resolution and TypeScript path mapping. Its
`ResolverGeneric<Fs>` accepts a filesystem implementation. However, its
default `Resolver` uses the operating system filesystem, which must not become
an implicit dependency of SyntaxMesh's runtime-neutral engine or core.

## Decision

1. Use the crates.io `oxc_resolver` implementation for the first ECMAScript
   module-resolution provider. Do not duplicate its Node/TypeScript path and
   package algorithms inside the language extractor or symbol resolver.
2. Introduce the resolution integration at the engine/application boundary as
   an injected provider. The provider receives the accepted extraction's
   typed import/re-export occurrences and indexed module inventory, plus
   explicitly selected project resolution settings. It returns deterministic
   resolution outcomes; it does not read or mutate the graph store.
3. Keep filesystem access outside `syntaxmesh-core`, runtime protocol, and
   the default resolver contracts. A host adapter may use Oxc's OS filesystem
   implementation; an embedded/in-memory host may supply an Oxc-compatible
   virtual filesystem. Equivalent hosts use the same provider/settings to
   obtain equivalent graph facts.
4. Resolve source imports and source re-exports to indexed `NodeKind::Module`
   nodes only. Add `RelationKind::ResolvesTo` from the typed occurrence node
   to the target module node. Do not rewrite the import/export node or claim
   member-level binding resolution. A specifier that resolves outside the
   indexed module inventory remains explicitly unresolved rather than creating
   an unowned or fabricated target.
5. Emit resolution evidence as a separate `EvidenceClass::StaticallyResolved`
   provenance record identifying the resolver producer/version, selected
   conditions/configuration fingerprint, source occurrence, and resolved
   target. Extracted source provenance remains unchanged. Ambiguous, invalid,
   and unresolved outcomes remain distinguishable; no terminal symbol-name
   fallback is permitted.
6. Recompute resolution for affected occurrences when the module inventory or
   resolver configuration changes, without reparsing unchanged source. Store
   updates remain ordinary atomic generation deltas through the existing
   Penelope publication path and node/edge temporal history. No new storage
   table or mutation workflow is introduced.
7. Keep source syntax extraction, module-path resolution, and symbol/member
   binding as distinct stages. Python and other language packs may register
   their own provider later; an absent provider means occurrences remain
   explicitly unresolved.

## Resolution profile for the first implementation

- Support relative paths, package self-reference/imports/exports, Node ESM and
  CommonJS conditions, and TypeScript path mappings according to the
  repository's selected `tsconfig`/package configuration.
- Preserve the configured resolution mode; do not silently choose one mode
  for mixed or ambiguous projects. Record the chosen settings fingerprint.
- Restrict resolved graph targets to modules present in the accepted index.
  Third-party package files not indexed by SyntaxMesh remain unresolved in the
  graph, even if Oxc can locate them on disk.
- Do not yet infer symbol-member bindings, compiler type relations, or
  cross-language targets.

## Alternatives considered

- **Hand-write extension, `index.*`, `package.json`, and `tsconfig` rules:**
  rejected because this forks mature resolver semantics and risks silently
  selecting the wrong target under NodeNext, Bundler, aliases, or package
  conditions.
- **Call Oxc's default OS-backed resolver inside the engine:** rejected
  because indexing behavior would depend on ambient filesystem access and
  would violate runtime-neutral embedding.
- **Resolve in Oxc extraction:** rejected because extraction is file-local and
  deterministic; resolution requires the whole repository, package metadata,
  and configuration.
- **Persist only a resolved path string on the import node:** rejected because
  it mutates source evidence, loses typed graph traversal/history, and couples
  extraction facts to a particular resolver/configuration version.
- **Create a parallel module-fact table/history family:** rejected because
  ADR-0066 already establishes typed source occurrence nodes and existing
  temporal edge history is sufficient for resolution changes.

## Consequences

- The engine gains an explicit optional module-resolution provider boundary;
  hosts choose or provide filesystem/config capabilities without changing
  core or runtime-protocol contracts.
- Resolution edges can be retracted/replaced atomically when repository files
  or settings change, while stable source occurrence IDs remain unchanged.
- Static module resolution is evidence, not a source fact. Provenance and
  history distinguish it from the unchanged extracted occurrence.
- External package targets not present in the canonical index remain
  unresolved for now; package graph ingestion and member binding are separate
  follow-up slices.

## Verification required

- Oxc resolver fixtures cover NodeNext ESM/CJS, extension and directory
  resolution, package `exports`/`imports`, and tsconfig path mapping using an
  injected filesystem; no test depends on ambient host files.
- Indexing fixtures prove unique matches add occurrence-to-module
  `ResolvesTo` edges, missing/ambiguous outcomes remain explicit, and imports
  never fall back to symbol terminal-name matching.
- A source node's ID, payload, source span, and extraction provenance remain
  byte-for-byte unchanged when a resolution edge is added, removed, or
  redirected by a settings/source-inventory change.
- Incremental fixtures prove a newly added/removed target or changed resolver
  fingerprint recomputes affected edges without re-extracting unchanged files.
- Shared backend conformance proves resolution provenance and edges survive
  restart, historical `GraphAt`, integrity checks, and all graph-store
  backends.

Implementation status (2026-09-28): `syntaxmesh-resolver` now defines the
runtime-neutral injected provider/request/outcome contract. The generic
filesystem `OxcModuleResolver` implements it; its identity includes the Oxc
resolver version and a fingerprint of the selected options. The Indexer and
embedded Engine accept an optional provider. When present, the indexer
re-resolves persisted and newly extracted import/re-export occurrences without
reparsing unchanged files, filters targets to the accepted module inventory,
and publishes separate `StaticallyResolved` provenance plus occurrence-to-
module `ResolvesTo` edges in the normal generation delta. Target removal
retracts the resolution edge without changing the source occurrence. The
indexer regression covers this behavior with a deterministic fixture provider;
the store conformance fixture checks the focused resolution-inventory query
across reference and durable backends.

The CLI integration remains partial. [ADR-0068](0068-cli-module-resolution-profile.md)
adds opt-in `[module_resolution] profile = "node"`; the CLI composes the OS
filesystem adapter at the host boundary and folds resolver identity/settings
into the generation identity. The request carries syntax mode: source
imports/re-exports use `node` + `import`, while `require()` uses `node` +
`require`. The Oxc fixture verifies conditional package exports for both modes,
and a CLI fixture verifies explicit enable/disable, resolution retraction, and
independence from StateChronicle verification.

Resolution outcomes are retained as typed temporal graph facts and exposed
through generation-pinned queries per
[ADR-0070](0070-versioned-module-resolution-diagnostics.md). The Node profile currently uses tsconfig
auto-discovery and a fixed source-extension set; Bundler and other toolchain
profiles are not implemented. The indexed module-resolution inventory and its
SQLite/Turso migration are specified in
[ADR-0069](0069-indexed-module-resolution-node-kinds.md), removing the prior
full-node-payload scan from Turso's routine resolver lookup. CLI end-to-end
fixtures verify resolved-edge persistence across a Turso restart, target
removal in the current generation, current diagnostic lookup after restart,
and retrieval of the earlier resolved edge through historical `GraphAt`.
Source occurrence identity and extraction-provenance preservation have an
indexer regression; byte-level payload comparisons across all durable backends
remain future conformance work.
