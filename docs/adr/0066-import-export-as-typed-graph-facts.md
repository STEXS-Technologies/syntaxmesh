# ADR-0066: Store imports and exports as typed graph facts

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0065 correctly identified the loss of binding and re-export semantics when
imports are flattened to generic `Reference`, but proposed separate canonical
fact families with parallel IDs, temporal indexes, snapshots, and migrations.
Inspection of the actual store model shows that source declarations already
have a canonical home: typed `NodeKind` values. Nodes already participate in
stable identity, provenance, file ownership, atomic deltas, generation roots,
history, checkpoints, `FactRef::Node`, and `GraphAt` across InMemory, File,
SQLite, and Turso.

Creating separate import/export fact families would duplicate those mechanisms
and create another synchronized projection. Shardline's migration discipline
is valuable when a schema transition is necessary; here the durable model can
be extended additively without new tables or a second history store.

## Decision

1. Keep `ImportRecord` and `ExportRecord` as typed language-SDK extraction
events. They preserve source syntax and provenance and do not resolve module
paths or symbols.
2. Lower each event to one canonical `Node` with an appended typed
`NodeKind::Import` or `NodeKind::Export`. Put the raw specifier, syntax form,
binding names, and type-only marker in that enum variant. Reuse `Node`'s stable
`NodeId`, source location, file owner, and provenance; do not add a separate
`FactRef`, history family, table, snapshot field, or serialized `Node` field.
3. Connect each import node from its containing module with `RelationKind::Imports`
and each export node with a new appended `RelationKind::Exports`. These edges
mean “this module contains this source occurrence”; they do not claim the
specifier resolved. A later resolver may add `ResolvesTo` edges from an import
or re-export occurrence to a module/binding node. Source nodes remain unchanged
when resolution changes. Absence of such a resolution edge leaves the source
occurrence explicitly unresolved.
4. Append the new `NodeKind` and `RelationKind` variants; never reorder or
reinterpret existing bincode enum ordinals. Do not change the serialized shape
of `Node`, `GraphDelta`, `GraphSnapshot`, or existing generation history
records. As with the existing appended `Class` node kind, source extraction
changes bump the producer semantic version so unchanged files are re-extracted.
5. Preserve per-binding occurrences: one import/export node per binding, with
its own exact source span. Static side-effect/default/named/namespace imports,
dynamic imports, CommonJS `require`, local/default exports, and named/
namespace/star re-exports use explicit typed forms. Keep module specifiers
exactly as written; package/path resolution is a separate repository-aware
stage.
6. Generic `Reference` remains for symbol/call occurrences. Module specifiers
must not be represented as generic symbol references or resolved by terminal
symbol-name matching.

## Alternatives considered

- **Separate Import/Export `FactRef` families and temporal tables:** rejected
  because existing typed `Node` history already provides identity-version
  lookup, roots, checkpoints, temporal queries, and persistence for every
  backend. A second fact family would duplicate all of those paths and require
  coordinated schema and snapshot migrations for no additional semantic power.
- **Store binding data in `ExtensionPayload` or encoded node names:** rejected
  because these are core language facts and must remain typed and queryable.
- **Keep generic references with a module-resolution exception:** rejected
  because it still loses binding aliases, re-export shape, and type-only
  semantics.
- **Resolve paths in the language extractor:** rejected because extraction is
  file-local while resolution depends on the complete repository and package
  configuration.

## Consequences

- The extraction contract gains typed import/export events and the canonical
  graph gains typed source-occurrence nodes; existing node history, temporal
  indexes, migration lifecycle, and query APIs are reused unchanged.
- SQLite/Turso need no new table migration, and legacy File/SQL histories keep
  their current serialized struct layouts. Additive enum variants preserve old
  ordinal meanings; current binaries must still fail closed if they encounter
  an unknown future variant.
- `NodeKind` is the authoritative source occurrence model. Do not additionally
  persist the same `ImportRecord`/`ExportRecord` as separate fact rows.
- This does not implement package resolution, alias-aware symbol binding,
  compiler-equivalent semantics, or full language extraction.

## Verification required

- [x] Extractor tests cover static default/named/type/namespace/side-effect
  imports, literal dynamic imports, CommonJS `require`, local/default exports,
  named/namespace/star re-exports, aliases, destructuring, exact binding spans,
  duplicate occurrences, and stable IDs under line movement.
- [x] Indexer validates source file/hash/span/provenance/module ownership and
  lowers each typed event to one typed node plus one module-ownership edge.
  Import/export nodes are excluded from the resolver's reference input by
  their `NodeKind`; Oxc tests also assert imports are not emitted as generic
  references.
- [x] Shared backend conformance covers publication, restart, node history,
  `GraphAt`, and canonical state across InMemory, File, SQLite, and Turso. No
  new storage-specific implementation or table migration was needed.
- [x] The resolver regression `import_specifiers_never_resolve_as_terminal_symbol_names`
  proves a module specifier such as `./utils` cannot match a function named
  `utils`. The indexer also excludes typed `NodeKind::Import` and
  `NodeKind::Export` from reference resolution. Because package/path resolution
  is not implemented, a future resolver still needs a regression proving that
  adding `ResolvesTo` edges leaves the canonical source occurrence unchanged.

Implementation status: typed SDK events, Oxc extraction, canonical `NodeKind`
variants, indexer lowering, resolver terminal-name exclusion, and cross-backend
history coverage are implemented. Module/package resolution and its future
source-occurrence immutability regression remain open.
