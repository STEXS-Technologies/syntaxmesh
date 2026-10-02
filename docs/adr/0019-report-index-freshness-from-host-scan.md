# ADR 0019: Report index freshness from caller-supplied source inventory

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

The store status reports the accepted generation but cannot tell whether it
matches the current source tree. The engine must remain runtime agnostic and
must not open paths, scan a worktree, or assume a filesystem watcher. The
scanner already gives hosts a deterministic inventory containing file IDs,
content hashes, normalized paths, and sizes.

## Decision

Add a runtime-neutral engine method that compares the accepted generation's
`FileVersion` inventory with a caller-supplied current inventory. Report counts
for files absent from the index, changed since indexing, and removed since
indexing. An empty set of differences means current. The caller is responsible
for supplying an inventory for the same repository/worktree scope; CLI status
with a source-root argument verifies that scope before comparison. Without a
source inventory, status reports freshness as not checked.

The comparison is a read-only snapshot diff, not a timestamp/latency metric,
watcher, or automatic indexing operation. CLI status with an explicit source
root reports stale state and exits non-zero when any difference exists.

## Consequences

- Embedded applications can report freshness using any runtime's own scanner
  or source inventory; the engine gains no host filesystem dependency.
- The CLI reuses the shared ignore-aware Rust scanner and the same root-derived
  repository/worktree identity used by indexing.
- A freshness check requires the caller to enumerate current source files.
  External inputs such as runtime observations and extension facts are not
  represented by source-file freshness.
- No persisted schema, generation identity, or canonical fact changes.

## Alternatives considered

- Scan the filesystem inside engine status: rejected because that couples the
  application core to a host/runtime and duplicates scanner policy.
- Infer freshness from the age of the generation: rejected because timestamps
  do not detect changed, newly added, or removed source files.
- Automatically re-index from status: rejected because a diagnostic read must
  not mutate canonical state.

## Migration and compatibility

This adds a read-only method and a freshness result type to the engine API. The
existing `status` CLI invocation remains valid and explicitly reports freshness
as not checked; an optional source-root argument enables the comparison.
