# ADR-0052: Bound Turso startup history validation

- Status: accepted
- Date: 2026-09-28

## Context

Turso startup validates the current canonical projection and manifest, but it
also decoded every retained `GenerationHistoryEntry` only to verify that the
latest entry matched the current manifest. That made open time and temporary
memory grow with total historical delta size, despite temporal queries being
served from indexed versions and persistent generation roots. A fixed-graph
benchmark across history depths 64, 256, and 1,024 confirmed the scaling.

The history remains the auditable source for rebuilds and StateChronicle
verification. Avoiding a full decode during routine open must not remove a
deep integrity path or weaken validation of the current published generation.

## Decision

1. Normal `TursoGraphStore::open` checks whether history exists and reads only
   the latest history row to compare its manifest with the current manifest.
   Current schema, indexes, canonical rows, and graph-root validation remain
   unchanged.
2. `BackendIntegrityCheck` scans retained history in sequence order, decodes
   each entry, and checks that its SQL generation key agrees with the payload
   manifest. This complements the physical database integrity check and
   checkpoint validation.
3. Historical payloads that are not needed to open the current graph may
   therefore be reported by the explicit integrity operation rather than
   preventing routine startup. A corrupt latest entry still fails open.

## Consequences

- Startup history validation becomes independent of retained history depth;
  canonical-root validation still costs at least a scan of current facts.
- Operators and maintenance workflows must run the explicit backend
  integrity check when they need deep validation of every retained history
  payload.
- Temporal query complexity and retained-history semantics do not change.
