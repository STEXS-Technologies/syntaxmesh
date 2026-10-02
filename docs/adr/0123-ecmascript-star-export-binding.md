# ADR-0123: Resolve named imports through indexed ECMAScript star exports

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0122 resolves an ECMAScript named/default import to a unique explicit
export in its resolved module. The Sim repository contains common barrel
patterns such as `packages/db/index.ts` (`export * from './schema'`) and
multi-file barrel chains. Stopping at the first barrel preserves module paths
but loses the imported binding even when every module and export occurrence is
already indexed.

The existing Node module provider resolves each export-from occurrence to an
indexed module and persists that path as `Export -> Module` `ResolvesTo` edges.
Reimplementing package/path resolution, parsing source again, or adding an
export table would duplicate existing mechanisms.

## Decision

1. Extend named-import binding to traverse `ExportKind::StarReExport` edges
from a resolved indexed module. Traverse only existing graph nodes/edges; do
not resolve paths or read files in the binding stage.
2. At each visited module, explicit exports matching the requested
`exported_name` take precedence. Only when no explicit match exists may
`export *` branches be followed. Star exports never supply the `default`
binding. Named/default re-export occurrences with explicit exported names
remain explicit public binding targets and are not recursively followed.
3. Collect distinct target `Export` node IDs deterministically across star
branches. Emit a binding only when exactly one target remains. A missing or
ambiguous result creates no import-to-export edge but preserves the valid
module-path edge. A visited-module set terminates cycles; a repeated path to
the same export node is deduplicated, while different export nodes with the
same name remain ambiguous.
4. Keep the graph shape from ADR-0122: the import retains its direct
import-to-module edge and receives at most one import-to-export `ResolvesTo`
edge. The target export may be declared in a module several star edges away.
Use version 2 binding provenance/edge identity and include the version in the
Node-profile CLI generation fingerprint so unchanged source is re-resolved.
5. Do not add compiler/type-checker equivalence. Duplicate declaration
occurrences, TypeScript declaration merging, namespace-import member use,
runtime conditional exports beyond the existing Oxc provider, and Python/Rust
binding remain outside this slice. No new dependency, public DTO variant,
store index, or migration is introduced.

## Alternatives considered

- **Resolve exports in the language extractor:** rejected because an extractor
  is file-local and cannot see indexed sibling modules or resolver results.
- **Repeat filesystem/path resolution during name binding:** rejected because
  the existing Oxc provider already owns Node/package/tsconfig resolution and
  publishes its indexed targets as graph facts.
- **Treat every same-named star candidate as one symbol:** rejected because
  SyntaxMesh does not have compiler symbol identity or declaration-merging
  semantics; only the same canonical export occurrence reached by multiple
  paths is deduplicated.
- **Add a dedicated relation or storage table:** rejected because the current
  typed Export nodes and temporal `ResolvesTo` edge model already represent
  this relationship and preserve its history.

## Consequences

- Imports through ordinary `index.ts` barrels and multi-hop `export *` chains
  can reach their unique source export without reparsing or extra provider
  calls.
- Explicit re-exports shadow star exports. Default imports are not inferred
  from stars, and cycles terminate without inventing a binding.
- Incompatible or ambiguous export surfaces remain conservatively unbound;
  SyntaxMesh does not claim TypeScript compiler equivalence.
- Existing stores and history remain generic: a new generation carries the
  binding edges and separate static-resolution provenance without migration.

## Verification

- A focused indexer test covers multi-hop traversal, explicit shadowing,
  ambiguous results, default exclusion, and cycle termination.
- The CLI Node-profile monorepo fixture resolves an aliased import through a
  two-hop barrel using TS path mapping and directory indexes; it checks
  ambiguous branches, explicit-over-star precedence, a cyclic missing name,
  and default-through-star exclusion.
- Editing the downstream module retracts the star binding but preserves its
  module edge; restoring the export restores the same occurrence binding.
- A Turso restart fixture verifies the star-chain module and binding edges
  survive in the historical generation after the barrel is removed; both
  edges are checked through the `graph-at-turso` CLI query.
- `cargo make ci` passes, including architecture, migration, lint, workspace
  tests, audit policy, and documentation generation.

## References

- [ADR-0067: Runtime-neutral module resolution provider](0067-runtime-neutral-module-resolution-provider.md)
- [ADR-0122: Exact ECMAScript export binding](0122-ecmascript-export-binding-resolution.md)
- [Sim database barrel](https://github.com/simstudioai/sim/blob/main/packages/db/index.ts)
