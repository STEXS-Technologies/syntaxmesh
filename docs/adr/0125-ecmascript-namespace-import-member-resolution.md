# ADR-0125: Resolve static ECMAScript namespace-import member uses

- Status: accepted
- Date: 2026-09-29

## Context

The Sim TypeScript corpus uses namespace imports for schema and library
modules, for example `import * as schema from '@sim/db/schema'` followed by
`schema.member` property access. SyntaxMesh already records the namespace
import and resolves it to an indexed module, but it does not preserve the
member-use occurrence or bind it to the corresponding exported occurrence.

The existing named-import binding, star-export traversal, module-resolution
provider, and typed import history should remain the only resolution path. A
terminal-name guess is unsafe because unrelated modules can export the same
member name.

## Decision

1. Append `ImportKind::NamespaceMember` without reordering existing serialized
enum variants. Record only a non-computed static member access whose immediate
object is a declared namespace-import local, such as `schema.member`.
2. The member-use record retains the original module specifier, the selected
   member name, local spelling `namespace.member`, the member-token span, and
   source provenance. It is an indexed typed occurrence, not an inferred
   compiler binding.
3. Resolve the member-use record through its already-resolved module using the
   existing exact-name export lookup and indexed `export *` graph. Preserve
   the module edge and add at most one export edge. Missing/ambiguous members
   remain unbound; computed keys and namespace aliases shadowed by another
   local declaration in the same file are conservatively omitted.
4. Keep imports from unrelated languages and named/default binding unchanged.
   No compiler/type-checker equivalence, arbitrary expression evaluation,
   computed property resolution, or package runtime execution is claimed.
5. The enum addition is append-only for existing bincode payloads. The stored
   node category remains `Import`; no table or migration is added. Bump the
   ECMAScript extractor and Node-profile generation fingerprints so existing
   source is re-extracted and re-resolved.

## Consequences

- Static `namespace.member` accesses become independently source-backed and
  can resolve through the same unique-export and star-barrel rules as named
  imports.
- The member-use node has enough exact source location to support future
  impact and usage queries without inventing a generic symbol guess.
- Shadowed aliases, computed properties, unresolved modules, absent exports,
  and ambiguous exports fail closed.

## Verification

- Core serialization test proves the enum variant is appended.
- Extractor tests cover aliases, repeated static member uses, computed access,
  and a locally shadowed namespace alias.
- CLI Node-profile tests bind a namespace member through indexed barrels,
  preserve unresolved/ambiguous member occurrences, and verify target removal
  retracts only the member binding while keeping the module resolution.
- Turso restart/`GraphAt` tests cover historical member-use and export-binding
  facts; full architecture, migration, format, lint, test, and documentation
  gates pass.

## References

- [ADR-0066: Typed import/export facts](0066-import-export-as-typed-graph-facts.md)
- [ADR-0122: Exact ECMAScript export binding](0122-ecmascript-export-binding-resolution.md)
- [ADR-0123: ECMAScript star exports](0123-ecmascript-star-export-binding.md)
- [Sim namespace-import example](https://github.com/simstudioai/sim/blob/main/apps/sim/lib/billing/authorization.ts)
