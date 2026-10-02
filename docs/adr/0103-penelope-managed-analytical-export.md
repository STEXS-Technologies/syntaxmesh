# ADR-0103: Penelope-managed analytical export workflow

- Status: accepted
- Date: 2026-09-29

## Context

The product source requires Penelope for durable multi-step work, including
DuckDB synchronization and long-running analytics, while keeping it off query
and parse hot paths. ADR-0098 makes analytical publication explicitly
downstream: an analytics failure cannot block or roll back a canonical graph
generation. The Parquet adapter now provides deterministic, restartable page
publication, but no Penelope workflow correlates its request and completion
with the engine's durable workflow records.

The integration should reuse the existing Penelope adapter patterns and the
Parquet export's own idempotent checkpoint. It must not move workflow concerns
into the analytics feed, Parquet crate, canonical store port, or core/public
contracts.

## Decision

1. Add an opt-in analytics-export workflow facade to
   `syntaxmesh-integration-penelope`. It operates on the existing
   `GraphStore + DurableRecordStore` and accepts a retained generation,
   destination, and bounded page size.
2. Persist a versioned Penelope process record, including the typed request,
   before writing artifacts. Bind the process identity and digest to
   repository, worktree, generation, absolute UTF-8 destination, and page size.
   Reuse is allowed only for the exact same request.
3. Treat the Parquet export as the idempotent external effect. If the process
   stops while pages are written or after export completion but before the
   workflow record completes, retry the same request; the existing manifest
   resumes or returns the completed artifact. Do not replay or materialize
   every historical generation.
4. Provide bounded `recover_pending()` over the durable-record cursor port. It
   validates and replays persisted Penelope events, then resubmits only
   prepared requests to the idempotent exporter. Completed processes are not
   re-exported.
5. Keep analytics workflow scheduling explicit and downstream. It is not
   coupled to graph generation publication, does not update the OLTP graph,
   and cannot change graph acceptance on failure. Hosts decide when to request
   synchronization; no daemon, timer, or background runtime is introduced.
6. Keep the adapter independent of DuckDB and service transports. A later
   DuckDB sync can consume the verified Parquet artifact through a separate
   optional adapter/workflow step. Hot query reads remain direct Rust calls.

## Alternatives considered

- Add Penelope to `syntaxmesh-analytics` or `syntaxmesh-analytics-parquet`:
  rejected because typed feed and artifact writing must remain usable without
  workflow orchestration.
- Put analytics export inside graph publication: rejected; rebuildable
  downstream analytics cannot delay or roll back canonical accepted history.
- Store every partition cursor only in Penelope: rejected; the Parquet manifest
  is already the bounded, page-level recovery checkpoint. The Penelope record
  correlates the long-running request and terminal result.
- Schedule exports automatically in a daemon: deferred; no daemon or
  background runtime is required for the local v0 slice.

## Consequences

- Full-engine hosts can explicitly run and recover durable analytics exports
  using their existing workflow journal.
- A completed workflow retry returns its recorded receipt. Artifact corruption
  remains detectable through the separate full integrity audit in ADR-0102.
- The Parquet format and store ports remain independent of Penelope. Analytics
  may still be run directly by tools that do not need durable scheduling.

## Verification

- A request is durably prepared before Parquet publication and completes only
  after the deterministic export reports completion.
- Reopening the durable store and retrying the same request returns the same
  receipt without adding partitions; changing any request-bound input fails
  closed.
- An export error leaves a replayable prepared workflow record and does not
  alter canonical graph state.
- The adapter uses no NDJSON staging or internal service protocol.
