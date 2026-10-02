# ADR-0091: Resume persistent adjacency reads after cache misses

- Status: accepted
- Date: 2026-09-29

## Context

The operation counters from ADR-0090 showed that historical-adjacency reads
loaded 18 incidence-tree pages but performed 182 page-cache lookups for a
fixed 10-edge request. Durable adapters retried `get` and ordered-range reads
from their roots whenever the next page was absent from the cache. The index
pages were bounded, but this repeated already completed search/traversal work.
The mutation side already avoids that failure mode with resumable cursors.

## Decision

1. Add resumable persistent-tree point lookup and ordered-range cursors that
   retain the exact search/successor position when they encounter a missing
   durable page.
2. Add an endpoint-root lookup cursor and an edge-ID page cursor over those
   primitives. SQLite and Turso load the requested page and resume the same
   cursor instead of rebuilding the search from the root.
3. Keep immutable keys, generation roots, page encoding, and result ordering
   unchanged. This is an internal query-work optimization: it requires no
   schema migration or wire/API record changes.
4. Preserve eager convenience helpers for fully cached callers by implementing
   them over the same resumable cursors.

## Consequences

- Cache misses no longer force repeated traversal of already examined paths.
- Read work remains proportional to search paths plus requested results; it is
  not claimed to be strict O(1) in total index size.
- Tests must prove lazy loading resumes correctly and ordered pages remain
  identical to the existing fully cached behavior.

## Verification

Persistent-tree tests exercise point and ordered-range reads with an initially
empty cache, supply each missing page lazily, and verify exact values/order.
The instrumented temporal benchmark validates unchanged 10-edge results and
records page-cache lookups, durable page loads, and edge payload rows. The
before/after comparison for the same fixed fixtures is recorded in
[ADR-0086](0086-generation-pinned-historical-neighbor-pages.md).
