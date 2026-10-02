# ADR-0080: Structurally share immutable fact-page cache snapshots

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0079 removed an avoidable second full-state clone while preserving the
detached-candidate → durable-commit → in-memory-publish sequence. The remaining
SQLite candidate clone still costs about 193 ms median for the one-file
incremental Shardline workload. A feature-gated breakdown of one representative
run attributed approximately 89 ms to `PersistentFactTreeCache`, 54 ms to
canonical facts, 39 ms to query indexes, and 11 ms to retained generation
history. These are clone-stage measurements, not exclusive workload totals.

The local `superscalar-kv` implementation uses immutable shared prefixes and
branch-local copy-on-write pages so speculative forks do not copy the full
prefix. Its dense slab/page-ID layout is domain-specific and does not fit
SyntaxMesh's content-addressed `StableId` fact pages. The transferable property
is structural sharing of ordered immutable page maps with path-local mutation.

## Decision

1. Use `imbl::OrdMap<StableId, PersistentFactNode>` for the in-memory
`PersistentFactTreeCache.pages` collection. It preserves the current ordered
key traversal and API-facing behavior while allowing clones to share immutable
tree structure and updates to copy only modified paths.
2. Keep the cache's newly created `dirty` set as a standard `BTreeMap`; it is a
   mutable, operation-local write set rather than a retained immutable snapshot.
3. Keep canonical file/provenance/node/edge maps, query indexes, serialization,
   and durable SQLite/Turso schemas unchanged. The persistent map is an internal
   cache representation only; page identity, canonical roots, and wire formats
   do not change.
4. Use the crates.io `imbl` package directly at a locked version. Do not depend
   on the sibling `superscalar-kv` crate: its allocator, dense IDs, page layout,
   and speculative KV API are not a fit for the graph store.
5. Remove only unreachable dirty page IDs during batch cleanup; do not rebuild
   the full persistent map through a whole-map `retain` operation.

## Alternatives considered

- Keep the cache as `BTreeMap`: rejected for candidate snapshots after the
  measured 89 ms clone cost.
- Wrap `BTreeMap` in `Arc` and use `Arc::make_mut`: rejected because the first
  candidate write would copy the entire map, merely moving the measured cost
  from clone to mutation.
- Copy `superscalar-kv`'s dense slab and fixed-block layout: rejected because
  SyntaxMesh pages are addressed by stable content hashes, not dense integer
  slots, and the sibling crate carries unrelated allocator semantics.
- Build a custom layered overlay map: rejected while a maintained generic
  ordered persistent map is available; custom compaction and lookup-depth rules
  would become SyntaxMesh-owned correctness/performance machinery.

## Consequences

- Candidate clones can share cached persistent pages, while each update creates
  only the changed ordered-tree paths.
- The cache may trade some single-owner mutation/build cost for cheaper forks;
  initial indexing and incremental publication both require measurement.
- The new dependency is MPL-2.0+, requires explicit license-policy inclusion,
  and stays private to `syntaxmesh-store`.
- Failed candidates may release their branch-local tree paths without
  modifying the committed cache snapshot.

## Verification

- Store tests pass (23/23), including root-oracle, deterministic ordering,
  corruption, lazy-page-load, rollback, migration, restart, and the new
  clone-isolation fixture. The clone-isolation test mutates a clone and checks
  exact ordered facts from both the original and candidate roots.
- `cargo deny check licenses` passes with the direct `imbl` dependency.
- Three fresh-database Shardline SQLite runs after the change measured median
  initial / one-file incremental indexing at 29.171 s / 711 ms, versus
  29.384 s / 950 ms in the immediately preceding three-run baseline. Retained
  database bytes were unchanged at 815,783,936; median peak RSS was 2,288.4
  MiB versus 2,260.2 MiB. Candidate store clone time fell from 193.387 ms to
  120.398 ms; the cache-map clone stage rounded to 0 ms in the instrumented
  runs. These are observed results from one local host/workload, not an SLA or
  proof that every workload benefits. Evidence is retained under
  `target/benchmark-results/repository-20260929T004301.496482Z-uncommitted/`.
- `cargo make ci` passed format, architecture, migration, check, Clippy, audit,
  and deny stages, but the workspace integration suite hit an existing
  environment-dependent failure: `git_context_reports_branch_head_dirty_and_detached_states`
  expects a Git repository, while this supplied workspace has no `.git`
  directory. It is unrelated to the map change; the complete suite therefore
  remains unverified in this environment.
