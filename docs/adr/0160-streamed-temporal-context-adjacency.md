# ADR-0160: Stream temporal context adjacency

- Status: accepted
- Date: 2026-09-30

## Decision

Reuse the existing sorted, generation/endpoint/direction-bound edge pages to
merge outgoing and incoming historical context adjacency incrementally. Keep
one page buffered per direction and deliver the lowest stable edge ID first;
on equal IDs deliver outgoing before incoming, matching the former stable sort
of concatenated outgoing/incoming vectors. Preserve duplicate self-edge visits
and existing selected-edge deduplication.

Keep current-only context on its existing adjacency ports and sorting semantics.
Selection, source verification, evidence classification, and packing remain in
the shared compiler. Introduce only a private sibling helper, not a public
streaming API, schema, scheduler, or dependency.

## Limits and verification

Historical temporary adjacency retains at most two edge pages, independent of
endpoint degree. This is an edge-count bound, not a byte bound: extension payload
size can vary. Selected parallel-edge retention and total edge scanning remain
unbounded by an independent work budget. Current adjacency remains vector-based.

Compare the merged stream against concatenation plus stable sorting for multiple
pages, interleaved directions, self edges, empty directions, and callback errors.
Verify candidate admission and existing current/historical context packs across
backends and restart, then run the full contributor gate.

The merged-stream oracle passes with 1,001 edges per direction plus a self-edge:
the first callback follows only two page reads, the complete stream uses four,
and its output matches concatenation/stable sorting including duplicate self
visits. Callback failure stops before later pages. Empty and single-direction
streams and saturated/admitted node selections also pass. Full `cargo make ci`
passes, including strict Clippy, historical/current context conformance across
all four backends and restart, and MCP server/stdio fixtures.

A real-adapter conformance fixture now publishes 1,001 parallel edges in each
direction plus a self-edge over the existing source fixture. It explicitly
checks the 1,000-entry first pages and remaining 3 outgoing / 2 incoming entries,
compares current and historical packs at candidate limits 1 and 2, deletes the
added edges in a later generation, and reopens File/SQLite/Turso stores to verify
retained and current evidence. This exercises multi-page adapter composition
rather than relying only on the mock routing test. The reference InMemory
compiler supplies the current-pack oracle; source classification and exact
serialized-byte budgeting are checked independently.
