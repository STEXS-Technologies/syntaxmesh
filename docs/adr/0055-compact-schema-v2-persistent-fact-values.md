# ADR-0055: Store compact values in schema-v2 persistent fact pages

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0054 defines the schema-v2 generation root from canonical JSON fact
commitments, while explicitly excluding page serialization from the public
root. The current implementation stores those full JSON bytes in every
content-addressed tree page. A three-run Shardline fixture (696 Rust files,
174,082 nodes, 217,693 edges) measured median retained sizes of 978,972,672
bytes for SQLite and 1,103,134,824 bytes for Turso. A follow-up `dbstat`
inspection attributed 389,844,992 bytes of SQLite pages to
`syntaxmesh_temporal_tree_pages`, versus roughly 159 MB in the prior
post-compaction sample; temporal fact versions remained about 103 MB. This
correlates the roughly 229 MB total-size increase with persistent tree pages,
but the old and new samples came from different uncommitted worktree states,
so it does not prove causation by itself.

Shardline's completed Penelope records provide a useful precedent: retain a
compact durable representation while binding it to the original semantic
request digest. SyntaxMesh can apply the same separation without changing the
canonical graph root.

## Decision

- Keep schema-v2 semantic value commitments byte-for-byte compatible: hash the
  canonical JSON fact bytes with the existing
  `persistent-fact-value-v1` domain. Generation manifests and all public graph
  roots remain unchanged.
- Store schema-v2 tree values as version-tagged bincode fact payloads, wrapped
  in a storage envelope containing a digest of the encoded payload. The
  envelope checksum detects damaged stored bytes; the canonical semantic
  commitment remains the page identity input.
- Keep schema-v1 root and payload semantics unchanged. Schema-v2 readers accept
  both existing raw canonical-JSON pages and the new enveloped bincode pages,
  so no root rewrite or SQL schema migration is needed. Existing content page
  IDs remain stable; if a page already exists in the old representation,
  `INSERT OR IGNORE` may retain it, and the reader handles that mixed history.
- Continue verifying reconstructed snapshots against their generation
  manifests. Physical payload digests and canonical graph commitments protect
  distinct layers and are both required.
- Preserve deterministic backend-independent tree construction. SQLite,
  Turso, and the reference root implementation must produce identical
  schema-v2 roots for the same facts.

## Alternatives considered

- Keep storing JSON in each tree page: rejected because benchmark attribution
  shows the persistent tree is a dominant retained-storage cost, while the
  public root already excludes storage encoding.
- Hash bincode instead of canonical JSON for schema v2: rejected because it
  would change the root contract and make backend/library serialization a
  source of public identity.
- Add a third root schema version and rewrite every retained root: rejected;
  this is a physical encoding optimization, not a semantic root change.

## Consequences

- Tests must prove canonical-JSON and compact-bincode mutations yield the same
  tree root, envelope corruption is rejected, old JSON schema-v2 pages remain
  readable, and SQLite/Turso/reference roots and historical reads still agree.
- The benchmark harness records per-table/index `dbstat` page and payload
  bytes, plus raw host/toolchain and timing evidence. Re-run the same workload
  after implementation before claiming any size or performance improvement.
- The one-run post-change Shardline verification retained matching graph roots
  and reduced the `syntaxmesh_temporal_tree_pages` allocation from
  389,844,992 to 177,094,656 bytes (payload 341,641,963 to 164,131,496 bytes).
  Total database size fell from the prior three-run medians of 978,972,672 to
  767,135,744 bytes for SQLite and 1,103,134,824 to 891,293,800 bytes for
  Turso. This is strong storage attribution from one post-change sample, not a
  repeated timing claim; timing was similar but has not been re-benchmarked
  over multiple post-change samples.
- Bincode remains an internal durable codec with the existing RustSec
  unmaintained advisory exception. A future codec replacement requires a
  separate compatible page-payload decoder/migration decision.
