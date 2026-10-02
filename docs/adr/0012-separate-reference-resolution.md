# ADR 0012: Separate reference extraction from resolution

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

The source-of-truth requires extractors to emit references without resolving
them, then resolve them in a separate stage. It also requires unresolved
references to remain explicit. The current Rust extractor instead resolves
call names against declarations from the same file and drops every target it
cannot match. This loses cross-file calls, unresolved calls, and the evidence
needed to improve resolution later.

## Decision

The language SDK will expose raw reference occurrences separately from
definitions and already-canonical graph edges. A reference records its source
symbol, target spelling, relation kind, and source evidence. Extractors do not
decide whether the target exists.

Add a pure `syntaxmesh-resolver` crate between extraction and delta creation.
Its first deterministic rule resolves a reference only when exactly one
definition matches the target's terminal name in the current generation. Zero
matches remain unresolved; multiple matches remain explicitly ambiguous and
must not be guessed. Resolution emits `Calls`/`References` edges. Unresolved
results are retained as `UnresolvedReference` graph nodes linked from the
source by `References`, carrying the target spelling, source owner, source
location, and extraction provenance. This uses the existing graph persistence
contract and is queryable without adding backend-specific tables.

This is a deliberately conservative initial resolver, not a claim of full Rust
name resolution. Imports, lexical shadowing, macros, trait dispatch, and
compiler metadata remain future resolver strategies. Add later strategies
behind the resolver crate without changing extractor responsibilities.

Import references are not symbol references: their module specifier must never
fall back to a terminal function/class-name match. Until a module-aware resolver
is added, `Imports` references remain explicit unresolved facts. The ECMAScript
pack emits file-level module nodes and source-backed import occurrences without
claiming path, package-export, alias, or re-export binding resolution.

## Consequences

- Changes the public language-SDK extraction contract and canonical `NodeKind`
  enum; downstream implementors must populate the new reference list and
  exhaustive enum matches must handle unresolved references.
- Moves call-edge creation from `syntaxmesh-lang-rust` into the resolver.
- Preserves unresolved and ambiguous source relationships rather than silently
  dropping them. This may increase graph node/edge counts.
- Keeps core and SDK host-, database-, workflow-, and runtime-independent.
- Requires tests for unique, missing, and ambiguous targets and for
  cross-file resolution and source-backed unresolved facts.

## Alternatives considered

- Resolve inside each language extractor: rejected because it couples parsing
  to repository-wide semantic rules and prevents cross-file resolution.
- Silently drop unknown targets: rejected because it violates the source of
  truth and makes missing knowledge indistinguishable from no reference.
- Persist unresolved references in adapter-specific tables: rejected because
  it fragments canonical state across backends and complicates conformance.

## Migration and compatibility

Append the new `NodeKind` variant to preserve existing bincode enum variant
indices. Existing stored graph nodes/edges require no table migration. New
extraction output changes graph roots and generations as expected on the next
index. Old workflow records remain unchanged; the resolver runs before a new
delta is prepared.
