# ADR-0122: Resolve exact ECMAScript imports to indexed export occurrences

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0066 preserves import/export syntax as canonical typed nodes, and ADR-0067
resolves module paths through an injected provider while deliberately leaving
member binding open. The v0.1 plan still lists alias-aware TypeScript and
JavaScript cross-file binding as Gate 5 work. The existing facts retain both
the imported/exported surface names and their local aliases, and indexed
module-path `ResolvesTo` edges already identify the target module. A second
hand-written package/path resolver or new binding table would duplicate
existing behavior.

## Decision

1. After an existing module provider uniquely resolves an ECMAScript import to
an indexed `NodeKind::Module`, bind only static `ImportKind::Named` and
`ImportKind::Default` occurrences to exactly one explicit export occurrence in
that module. Match the import's `imported_name` to the export's
`exported_name`; never match the local aliases.
2. Eligible targets are explicitly named `ExportKind::Local`, `Default`,
`NamedReExport`, or `NamespaceReExport` occurrences. Exclude star re-exports
without an explicit exported name. Do not recursively chase re-export chains
in this slice.
3. Reuse `RelationKind::ResolvesTo` for the additional import-to-export edge.
The existing import-to-module edge remains. Consumers distinguish the module
resolution target from the exported binding by target `NodeKind`; consumers
must not assume an import has at most one `ResolvesTo` target.
4. Emit separate `StaticallyResolved` provenance using the exact import source
span and a versioned SyntaxMesh binding producer identity. Leave source
Import/Export nodes, IDs, source provenance, and parser output unchanged.
Missing or multiply declared export names produce no binding edge; they do not
invalidate a valid module-path edge or masquerade as path-resolution
diagnostics.
5. Reuse the indexer's existing resolver-owned edge replacement/retraction
path. Include an explicit binding-rule version in the CLI generation
fingerprint when the Node profile is enabled so a rule change re-evaluates
unchanged source. No schema migration, new public DTO variant, or new runtime
dependency is introduced.
6. Keep namespace imports, dynamic imports, CommonJS member binding, Python and
Rust binding, `export *` traversal, and compiler/type-checker semantics out of
scope. Continue using the existing Oxc Node/TypeScript provider for path
resolution and the indexed module inventory as the ownership boundary.

## Alternatives considered

- **Add a new relation or binding fact family:** rejected for the first slice.
  Existing `ResolvesTo` already represents statically resolved graph edges and
  existing graph/history/store projections can retain the extra target with no
  schema or DTO expansion. Target node kind supplies the endpoint distinction.
- **Match local aliases:** rejected because aliases are importer-local names,
  not the export surface contract.
- **Resolve imports directly to implementation declarations:** deferred. Export
  occurrences are the canonical public binding boundary; following aliases,
  re-export chains, overloads, and type/value namespaces needs separate rules.
- **Implement package and tsconfig path logic in SyntaxMesh:** rejected;
  ADR-0067 already delegates those semantics to `oxc_resolver`.

## Consequences

- Named/default TypeScript and JavaScript imports can navigate to a unique
  corresponding export occurrence, including import and export aliases.
- Every resolved import retains both the module-level edge and (when unique)
  the member-level edge; generic graph clients should inspect endpoint kinds.
- This remains source-level export binding, not proof of type correctness or
  runtime behavior. Ambiguous/missing names are represented by absence of a
  binding edge while the original source facts remain available.
- Existing graph stores, historical queries, and serializers carry the new
  edges and provenance through their current generic mechanisms.

## Verification

- Extractor-backed fixtures cover imported alias versus exported alias,
  default imports, a missing export, and an ambiguous exported name.
- Indexing proves the import keeps its module edge, adds only the unique
  matching export edge, and preserves source node/provenance identity.
- Re-indexing after export changes/removal retracts stale binding edges and
  restores them when the unique export returns.
- CLI Node-profile coverage exercises a representative Sim-style path alias
  and aliased import/export using the existing Oxc module provider.
- Run backend conformance and restart/history coverage to confirm the generic
  edge representation needs no store migration.

## References

- [ADR-0066: Typed import/export source facts](0066-import-export-as-typed-graph-facts.md)
- [ADR-0067: Runtime-neutral module resolution provider](0067-runtime-neutral-module-resolution-provider.md)
- [ADR-0068: CLI Node module-resolution profile](0068-cli-module-resolution-profile.md)
- [Sim aliased imports](https://github.com/simstudioai/sim/blob/main/apps/sim/executor/index.ts)
