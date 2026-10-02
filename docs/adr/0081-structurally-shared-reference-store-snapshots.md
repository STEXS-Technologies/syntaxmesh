# ADR-0081: Structurally share reference-store candidate snapshots

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0079 made graph-delta staging single-clone and ADR-0080 made the immutable
fact-page cache structurally shared. A new publication-scaling benchmark
measured five samples for synthetic graphs with 100, 1,000, and 5,000 nodes
(2N total canonical facts) and deltas replacing 1, 10, or 100 nodes. One-fact
median publication rose from 9.9 to 51.1 microseconds in InMemory, and from
1.19 to 5.50 milliseconds in SQLite, while Turso remained between 1.50 and
1.83 milliseconds. This is consistent with the reference and SQLite candidate
paths copying their `InMemoryGraphStore` maps in proportion to graph size;
the timings are a local synthetic signal, not a causal attribution or SLA.

`InMemoryGraphStore::clone` still deep-copies canonical `BTreeMap` collections
and derived lookup indexes before applying a candidate. `superscalar-kv`
demonstrates the transferable design principle of immutable shared structure
with branch-local path updates. Its dense slab/page IDs do not fit SyntaxMesh;
ADR-0080 already selected the maintained `imbl::OrdMap` implementation for
SyntaxMesh's ordered, content-addressed pages.

## Decision

1. Use `imbl::OrdMap` for the reference store's canonical keyed maps
   (`files`, `provenance`, `nodes`, `edges`, and durable opaque `records`) and
   its derived ordered lookup maps. Keep each per-key edge/node membership set
   as `BTreeSet`; only touched sets are copied when a candidate updates an
   index key.
2. At this decision point, leave append-only vectors unchanged; subsequent
   history-clone profiling justified the separate, narrower follow-up in
   [ADR-0082](0082-structurally-shared-generation-history.md). Lineage and
   consequence journals remain `Vec` because their clone cost has not been
   isolated as material.
3. Preserve deterministic ordered iteration and the existing Serde map wire
   layout. Enable `imbl`'s Serde support and add bidirectional Bincode
   compatibility tests between `BTreeMap` and `OrdMap` encodings before using
   the new map types in File snapshots. Legacy snapshot DTOs remain
   `BTreeMap`-based and convert into the current map representation on load.
4. Do not change public DTOs, SQL schema, canonical fact/root semantics, or
   publication ordering. Candidate state remains isolated until durable commit
   succeeds; failed candidates cannot mutate the published snapshot.
5. Keep the change only if clone isolation, legacy snapshot compatibility,
   cross-backend conformance, and the same publication-scaling matrix pass, and
   measurements show that single-fact publication is less sensitive to total
   graph size without an unacceptable initial-index regression.

## Alternatives considered

- Keep deep-cloned `BTreeMap`s: rejected for measured candidate paths whose
  single-fact cost rises with total graph size.
- Wrap each `BTreeMap` in `Arc` and call `Arc::make_mut`: rejected because the
  first candidate write copies the entire map instead of only the modified
  search paths.
- Use the sibling `superscalar-kv` dense arena directly: rejected because its
  key/allocator model is specific to dense KV slots rather than typed stable
  fact identities.
- Implement a SyntaxMesh-specific overlay map: rejected while the maintained
  `imbl::OrdMap` is already used for the persistent fact-page cache.

## Consequences

- Candidate clones can share canonical map and index structure; updates copy
  only touched paths and touched per-key sets.
- Single-owner mutation and full-index rebuild costs may change and must be
  measured alongside candidate publication.
- File snapshot field ordering, canonical roots, SQL storage, and old snapshot
  readability remain compatibility requirements, not assumptions.
- InMemory and SQLite remain reference/conformance backends; this does not
  alter Turso's direct durable transaction path.

## Verification

- `cargo test -p syntaxmesh-store` passes (24 tests at the map-only stage),
  including the BTreeMap/OrdMap Bincode byte-identity test and legacy snapshot
  migration/restart fixtures. SQLite's 36 tests and all ten Turso backend
  conformance tests also pass.
- After this change plus the history follow-up in ADR-0082, three fresh SQLite
  Shardline runs measured median initial / one-file incremental indexing at
  29.586 s / 622 ms, versus 29.171 s / 711 ms before. Candidate-store clone
  time fell from 120.398 ms to 18 microseconds; manual store-clone profile
  groups for generation history, canonical facts, indexes, and persistent
  cache each measured at or below 6 microseconds. Median peak RSS was 2,286.0
  MiB versus 2,288.4 MiB, and retained database bytes were unchanged at
  815,783,936. Initial indexing did not regress materially in this comparison.
  Evidence: `target/benchmark-results/repository-20260929T010710.120719Z-uncommitted/`.
- The five-sample synthetic publication matrix at graph sizes 100/1,000/5,000
  nodes is recorded in the v0 plan. It measures GraphStore publication for
  1/10/100 updated nodes; it is a single local run and not an SLA.
