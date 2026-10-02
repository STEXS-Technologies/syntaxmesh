# ADR-0054: Version canonical generation roots for incremental publication

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0030 makes durable historical graph selection independent of history depth
with structurally shared roots. A separate cost remains on the publication
path: schema-v1 `GenerationManifest.graph_root` is computed by serializing all
current nodes and edges in canonical order for every generation. SQLite and
Turso also perform a whole-projection reference check before commit. The SQL
row writes and persistent-tree updates are delta-scoped, but these checks make
accepted one-fact changes O(F) in the existing graph size.

The public manifest root is checked across InMemory, File, SQLite, and Turso,
retained history, checkpoints, and optional StateChronicle verification. It
cannot be replaced by a backend-local root-page ID: that would make public
identity depend on database encoding, and it would omit a specified canonical
algorithm. Existing v1 manifests must remain verifiable after an upgrade.

## Decision

- Preserve `schema_version: 1` root semantics exactly for existing manifests:
  BLAKE3 over the generation identity followed by canonical JSON encodings of
  nodes and edges in stable-ID order. Do not rewrite accepted v1 manifests.
- Define schema-v2 `graph_root` as a domain-separated commitment over the
  generation identity and the semantic root of a deterministic persistent
  fact tree. The v2 tree covers canonical file, provenance, node, and edge
  facts ordered by `(fact_kind, stable_fact_id)`. Every fact value is committed
  using its canonical JSON representation; internal page serialization,
  database page IDs, SQL sequence numbers, and tree traversal order are not
  part of the public root.
- The semantic page commitment includes the fact family, stable fact identity,
  canonical value commitment, and semantic commitments of its left and right
  children. The page address used for persistence may additionally bind the
  stored representation and physical children. This keeps public identity
  independent of backend encoding while retaining content-addressed storage.
- A v2 publication derives its semantic root by applying only the accepted
  delta to the parent root. Routine publication validates the delta and the
  references it changes (including references affected by removals); a full
  canonical-root and referential audit remains available through explicit
  integrity checks. Open-time validation remains fail-closed and may be O(F).
- Version the root algorithm in `GenerationManifest.schema_version`. Readers
  dispatch validation by that version. During upgrade, retain v1 roots and
  manifests unchanged; build/validate the v2 root needed for the first v2
  publication from the validated current state. Any migration or later
  backfill of retained v2 roots must run transactionally, validate against the
  corresponding manifest, and leave the old database usable on failure.
- All reference and durable backends must produce the same v2 root for the
  same accepted canonical facts. StateChronicle remains optional verification
  of manifests and does not change root calculation or indexing behavior.

## Consequences

- The shared persistent-fact tree now derives a backend-neutral semantic
  commitment; v2 leaves are canonical JSON facts across file, provenance, node,
  and edge families. New InMemory, SQLite, and Turso publications use schema 2;
  v1 values/manifests remain readable and are not rewritten.
- SQLite and Turso bootstrap the first v2 generation from the validated current
  snapshot after a v1 history, then apply later deltas to the shared root.
  Historical reads dispatch by each retained manifest's schema version.
- Open-time v2 validation checks both the current projection root and equality
  between that projection and the retained persistent fact tree. Routine writes
  remain delta-scoped; open/integrity checks may scan all facts.
- Existing tests cover deterministic/shared persistent roots, retained
  historical reads, migration anchors, reopen, and backend persistence.
  Targeted SQLite and Turso fixtures now construct an actual retained schema-v1
  root, reopen it, publish the first schema-v2 child, and verify both versions
  across a second restart. This exercises the explicit fresh-root rebuild path
  separately from ordinary parent-root incremental publication. SQLite also
  has a targeted corruption fixture proving a damaged persistent-root page
  fails historical reads and is reported by the explicit backend integrity
  check.
- Keep the one-time v1-to-v2 root bootstrap O(F); subsequent accepted
  publications should be proportional to changed facts and tree depth. This
  does not make full `GraphAt` constant-time: producing its result remains
  O(output size). SQL index seeks are not claimed to be strict O(1).
- Measure fact-count, changed-fact-count, SQL/page writes, and storage growth
  independently. Root publication now avoids the prior whole-graph root
  serialization after bootstrap; do not claim a specific asymptotic or latency
  improvement until representative benchmarks include reference validation,
  tree writes, and backend projections.
- Keep SQLite's explicit migration lifecycle and checksummed migration ledger
  (ADRs 0046 and 0051); schema migration is not an implicit side effect of
  opening a store.
