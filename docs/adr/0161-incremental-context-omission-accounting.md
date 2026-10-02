# ADR-0161: Incremental context omission accounting

- Status: accepted
- Date: 2026-09-30

## Decision

Reuse the packer's existing reversible trial-admission semantics for omission
counts. Count deduplicated candidates once, then subtract only the trial item's
class. Commit the trial counters only when the item fits; rejection or tokenizer
failure leaves accepted counts unchanged. Remove repeated whole-candidate scans
and the included-key set from the admission loop.

Keep counters as `usize` until serialization into the existing saturating `u32`
DTO fields. This preserves saturation behavior even when raw counts exceed the
DTO maximum. No public DTO, token-counting rule, ordering, or evidence change.

## Verification

Compare counters with the former scanning oracle for mixed accepted/rejected
classes and saturation boundaries. Retain exact serialized-budget and tokenizer
rollback tests. The multi-page real-adapter fixture supplies current/historical
parity, candidate saturation, edge deletion, and restart evidence. Its initial
87-second run motivates inspecting packing work, not a database latency claim.

The mixed-class scanning oracle, saturation tests, and complete `cargo make ci`
gate pass. A subsequent isolated all-feature fixture run passes in 89.27 seconds
with opt-in query-phase timers (`SYNTAXMESH_STORE_PROFILE=1`). For the dense
generation, capacity-one historical packs take approximately 1.16 seconds on
InMemory/File, 0.11 seconds on SQLite, and 5.92 seconds on Turso; capacity-two
packs take 1.73, 1.73, 0.53, and 6.32 seconds respectively. Current capacity-two
packs take approximately 0.43–0.47 seconds across those adapters. These are
single-run debug-build fixture measurements, not representative latency or an
omission-counter speed comparison. They identify Turso's dense historical reads
and per-candidate packing as separate follow-up targets. Reference profiling
also confirms repeated replay in its default historical read implementation;
that behavior must not be attributed to the indexed durable adapters.
