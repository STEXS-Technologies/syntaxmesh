# ADR-0352: Engine inference application service

Status: opt-in engine service implemented; parent setup recovery completed by
ADR-0353; host command delivery pending.

## Decision

Expose opt-in inference composition from `syntaxmesh-engine`, not orchestration
duplicated in CLI/HTTP/MCP. Keep pure facts, public transport DTOs and thin runtime
protocol dependencies unchanged. Every complete rebuild, stream initialization
and successor effect goes through the existing Penelope coordinators. SQLite
migration remains explicit and separate from normal service calls.

Add mutable-reference forwarding to canonical `GraphStore` and
`DurableRecordStore` ports, using compile-time `auto_impl` 1.4. Forward every
method, including indexed backend overrides, rather than inheriting generic
fallback scans. This permits existing owned coordinators to borrow the engine's
store without copying journal logic, discarding engine configuration or changing
existing serialized requests. The macro has no runtime component; its forwarding
behavior is documented in [auto_impl](https://docs.rs/auto_impl/1.4.0/auto_impl/).

Under `syntaxmesh-engine/inference-sqlite`, expose initialization from a bounded
`RebuildRequest` plus positive page budget; first recover complete-baseline jobs,
then stream initialization/successors. Validate engine repository/worktree scope
before publishing a requested effect. Initialization returns the existing
`StreamOrigin`; synchronization returns the existing `StreamCheckpoint`.
Recovery reports separate complete-baseline and stream-effect counts.
Recovery is store-wide for existing journaled jobs, consistent with the existing
coordinators; scope checks apply to new requested engine effects.
[ADR-0353](0353-durable-inference-setup-intent.md) adds parent setup intent before
either child preparation, closing the gap between individually journaled effects.
Recovery reports parent setups separately from baseline and stream effects.
Keep database destinations explicit. Do not silently skip canonical generations,
rebuild changed configurations, introduce transport/runtime policy or replace the
canonical StateChronicle publication/verification path.

## Required evidence

Borrowed ports preserve optimized backend methods and durable CAS semantics.
Engine setup/recovery/synchronization works with File and Turso and survives
reopen, including wrong-scope rejection without a new effect. Existing journal
bytes, pure dependency boundaries and engine indexing behavior remain unchanged.
Host commands and transport DTO delivery remain unfinished until integrated and
tested separately.

## Implemented evidence

The engine now composes complete-baseline rebuild, stream initialization,
successor synchronization and dependency-ordered recovery using its borrowed
canonical store. New requested effects check engine scope before preparing any
inference journal; zero initialization page admission rejects before baseline
work. Existing engine configuration and canonical verification paths are retained.

Nineteen feature-enabled engine tests and strict all-target Clippy pass. File and
Turso fixtures exercise scope rejection without a journal, borrowed generation
coordinates, retraction/restoration, setup retry after an advanced head, recovery
with no pending work and reopen. The architecture gate passes. This is targeted
verification after the preceding full CI snapshot; no subsequent full CI is
claimed. ADR-0353 separately verifies whole-command setup recovery; user-facing
command delivery remains unfinished.

All 31 existing feature-enabled Penelope integration tests still pass after
borrowed-port support, including exact journal-byte compatibility. Dependency
policy checks admit the new compile-time macro dependency; existing allowed
license-metadata/yanked warnings remain. `git diff --check` is clean.

Subsequent full contributor CI passed on 2026-10-09 in 293.87 seconds, including
the ADR-0353 setup coordinator and ADR-0354 retained read service. Host command
delivery remains pending.
