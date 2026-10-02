# ADR-0065: Preserve typed import and export source facts

- Status: superseded by [ADR-0066](0066-import-export-as-typed-graph-facts.md)
- Date: 2026-09-28

## Context

The SyntaxMesh source of truth §9.1 defines `ImportRecord` and `ExportRecord`
as distinct language-extraction events. The current SDK instead lowers module
dependencies immediately to generic `Reference` occurrences. This preserves a
specifier and source span, but loses imported/local names, aliases, re-export
shape, and type-only status. It also leaves the resolver unable to distinguish
module lookup from symbol lookup except by a special-case relation guard.

I found no directly reusable, language-neutral import/export fact contract in
the sibling projects checked. The reusable storage patterns are already
established in SyntaxMesh:
typed canonical fact payloads and indexed history (ADR-0033), durable ordered
migrations (ADRs-0037/0040), and row-authoritative Turso storage (ADR-0024).
These patterns descend from Shardline's explicit migration discipline; they
must be reused rather than adding parser-specific persistence or an opaque
extension payload.

## Decision

1. Add typed `ImportRecord` and `ExportRecord` extraction outputs to the
   language SDK. They are source facts and do not perform module or symbol
   resolution. Preserve the exact source location and producer provenance.
2. Model imports per binding/occurrence, with a typed form (side-effect,
   default, named, namespace, dynamic, or CommonJS), the raw module specifier,
   optional imported and local names, and a type-only flag. Model exports per
   occurrence, with a typed form (local, default, named re-export,
   namespace re-export, or star re-export), optional source specifier,
   exported/local names, and a type-only flag. A statement with multiple
   bindings yields multiple records; each record retains its binding's source
   span. Stable IDs use module identity plus semantic shape and a deterministic
   duplicate disambiguator, never line/byte offsets as the primary identity.
3. Persist these records as first-class canonical fact families, not as
   `Reference` nodes, names encoded into strings, or `ExtensionPayload` bytes.
   Add typed IDs, `FactRef`/`FactPayload` variants, generation delta/snapshot
   fields, validity-indexed history, integrity/root participation, and query
   access. This makes import/export syntax historically inspectable even when
   resolution changes later.
4. Import/export records remain distinct from derived graph edges. The module
   resolver may later add or retract resolved `Imports`/`Exports` edges while
   retaining the original source records unchanged. Until that resolver is
   implemented, these records stay explicitly unresolved; module specifiers
   must never fall through to terminal symbol-name matching.
5. Existing durable formats must remain readable. Because generation history
   and snapshots contain bincode payloads, do not simply append fields to
   legacy serialized structs and assume compatibility. Add versioned decode
   types/transformations and ordered SQLite/Turso migrations, using the
   existing explicit migrate-before-open lifecycle. Fresh schemas and upgrades
   must converge on the same schema. File snapshots require an explicit
   versioned compatibility path as well. Append any new serialized enum
   variants without reordering existing ordinals. No store may silently discard
   import facts during recovery or history reconstruction.
6. Extractors that do not yet emit typed records continue to provide their
   existing facts. Generic `Reference` remains the contract for symbol/call
   occurrences; it is not used as a fallback representation for imports.

## Alternatives considered

- Keep generic references and add importer-specific metadata to their target
  string: rejected because it is untyped, ambiguous, and not queryable as
  binding semantics.
- Put serialized records in `ExtensionPayload`: rejected because these are
  core language facts, not extension-owned data, and opaque payloads defeat
  canonical history queries and integrity checks.
- Emit typed SDK events but drop them before persistence: rejected because it
  would satisfy only the parser boundary while losing the facts needed by the
  versioned graph and future resolver.
- Resolve module paths during parsing: rejected because extraction must remain
  deterministic and file-local; resolution depends on the repository inventory
  and package configuration.

## Consequences

- This is a public canonical-fact contract change and requires additive schema
  migrations across File, SQLite, and Turso plus shared backend-conformance
  coverage. The implementation must not change the current generation identity
  or validity semantics for existing facts.
- Typed source facts make later module resolution, alias-aware references,
  re-export traversal, and historical import changes build on one durable
  contract instead of reparsing old source.
- Oxc becomes the first producer. Python and Rust import extraction remain
  separate follow-up slices and may use only the forms their syntax supports.
- This decision does not claim package/path resolution, compiler-equivalent
  semantics, or complete ECMAScript extraction.

## Verification required

- SDK fixtures cover each import/export form, aliases, multiple bindings,
  type-only syntax, exact source spans, stable IDs under line movement, and
  distinct duplicate occurrences.
- Shared store fixtures prove add/update/remove, restart, canonical-root
  validation, `fact_history`, `GraphAt`, and full integrity for both fact
  families on InMemory, File, SQLite, and Turso.
- Migration fixtures upgrade pre-change SQLite/Turso databases and File
  snapshots, then compare current and historical graph results with their
  pre-upgrade logical state. Interrupted/failed migrations must leave the
  previous store readable and retryable.
- Resolver fixtures prove import specifiers cannot resolve by terminal symbol
  matching and that adding module resolution later does not rewrite source
  records.

Implementation status at supersession: typed source-record structs and IDs
were added to `syntaxmesh-core` as scaffolding. ADR-0066 replaces the separate
canonical fact-family persistence approach with typed graph nodes that use the
existing node history and store contracts.
