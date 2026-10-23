# ADR-0397: Indexed fact observation bounds

- Status: accepted
- Date: 2026-10-23

## Context

The v5 source-of-truth requires `FirstObserved` and `LastObserved`. The current
observation timeline only pages all fact versions in a producer-time range. A
caller asking for the bounds of one known fact would otherwise need to read its
entire retained history or scan the global timeline. Producer-reported
`observed_at` is distinct from engine acceptance time and does not prove that a
runtime executed a behavior.

## Decision

Add a generation-independent query for the minimum and maximum producer
observation timestamp attached to one exact canonical `FactRef`. Return no
bounds when no retained version of that fact has an observation timestamp.
Expose both values as a versioned temporal record and CLI operation. SQLite and
Turso use an index ordered by `(fact_kind, fact_id, observed_at,
valid_from_sequence)` to seek the first and last entries; reference stores may
scan the selected fact's versions. The result is about the retained versions
currently available, not proof of complete external runtime history.

## Alternatives

- Reuse `observed_between`: rejected because it requires an arbitrary range and
  traverses the global time index.
- Read `fact_history` and compute the bounds in Query: rejected for durable
  stores because work grows with versions of that fact.
- Call a timestamp runtime-observed behavior: rejected because producer event
  time alone does not establish runtime execution.

## Consequences

This adds an additive temporal record and CLI command, plus one additive SQLite
and Turso migration. Existing timeline records and schema meanings remain
unchanged. A future selector-level runtime query must establish its own
behavior-to-observation evidence mapping and must not silently broaden this
exact-fact operation.
