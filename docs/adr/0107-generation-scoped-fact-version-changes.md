# ADR-0107: Read exact fact-version changes at one accepted generation

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0106 provides bounded accepted-generation pages, but a downstream
materializer must also know the exact versions opened and closed by a
transition. Calling `fact_history(fact)` for every changed fact returns its
entire lifetime and repeats work as histories grow. Reading adapter-private
fact-version tables from DuckDB would bypass the store port. The canonical
temporal fact table already keeps each version's start and exclusive end
sequence and an identity/start index.

SyntaxMesh's v5 architecture requires a rebuildable analytical projection
which is downstream of canonical commits. The typed Arrow/Parquet export is
the full rebuild path; Penelope coordinates resumable effects. Shardline's
rebuild pattern is reused at the right boundary: reconciliation is explicit,
retries are idempotent, and incomplete scans must not authorize destructive
cleanup. This decision adds a narrow typed store read, not DuckDB writes during
graph publication.

## Decision

1. Add `GraphStore::fact_versions_changed_at(fact, generation)`. It returns
   only exact canonical versions of `fact` whose `valid_from` or `valid_until`
   is the requested accepted generation. An insertion returns one version, an
   update normally returns the closed and opened versions, a deletion returns
   the closed version, and an unchanged fact returns no versions.
2. SQLite/Turso resolve the generation sequence and perform an identity-scoped
   indexed lookup over start and end sequences. Add a composite
   `(fact_kind, fact_id, valid_until_sequence, valid_from_sequence)` index for
   closure lookup using each adapter's registered migration lifecycle.
   InMemory/File retain a clearly documented history-scan reference path.
3. Incremental analytics consumes accepted changes and these typed versions,
   converts them through the existing Arrow attribute schema, and commits
   changed fact-version rows plus its repository/worktree generation watermark
   in one DuckDB transaction. The watermark advances only after the full
   bounded source page has been applied.
4. Verified Parquet remains the complete build/repair input. A failed or dirty
   rebuild never prunes/replaces a materialization; only a successfully
   verified staged rebuild may replace it. Penelope journals sync/rebuild
   requests and retries; DuckDB failure never affects graph publication.
5. No NDJSON, JSON text, service protocol, DuckDB private SQL access to
   SQLite/Turso, or workflow/database dependency is added to core/store ports.

## Alternatives considered

- Read a fact's complete version history for each transition: rejected because
  catch-up cost grows with versions older than the sync range.
- Have DuckDB open adapter-private SQLite/Turso tables: rejected because it
  couples analytics to migrations and bypasses the public store contract.
- Re-export/scan all Parquet history on every incremental sync: rejected as
  O(total history) work in place of delta-proportional synchronization.
- Write DuckDB synchronously inside canonical graph publication: rejected as
  dual-write consistency coupling.
- Treat a generation delta as the queryable history graph: rejected by
  ADR-0036; event/fact-version lineage and typed temporal dimensions remain
  separate.

## Consequences

- One identity/generation lookup returns at most the versions started or ended
  at that sequence; durable lookup work is bounded by index depth plus output.
- The additive store-port method requires conformance coverage and additive
  SQLite/Turso migrations; it does not itself implement DuckDB synchronization.
- Full materializer work remains open: changed-fact paging, transactional
  upsert/watermark, replay recovery, staged rebuild replacement, and measured
  depth-independent query behavior.

## Verification

- Insert, update, delete, unchanged, and anchor-boundary cases agree across
  InMemory, File, SQLite, and Turso.
- SQLite/Turso query plans use identity/start and identity/end indexes.
- Migration status remains read-only; explicit migrate is required before
  opening pre-existing databases.
- Existing publication and DuckDB paths remain downstream and independent.
