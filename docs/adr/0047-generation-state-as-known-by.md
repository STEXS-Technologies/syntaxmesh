# ADR-0047: Generation state qualified by acceptance cutoff

- Status: accepted
- Date: 2026-09-28
- Implementation: complete for exact retained-generation queries using the
  engine-acceptance axis; generic calendar-valid-time reconstruction remains
  outside this decision.

## Context

ADRs 0027 and 0031 require historical graph reads to avoid replaying every
accepted delta and distinguish modeled generation validity from engine
knowledge/acceptance time. SyntaxMesh already resolves a generation's graph
root directly and records optional acceptance time per generation. A correct
knowledge-cutoff check must consider every transition in the generation's
parent chain. Checking only the selected generation's timestamp is incorrect
if a runtime clock moves backward; scanning its ancestors at read time would
make query cost grow with history depth. Legacy generations may have unknown
acceptance time, which must remain unknown rather than be fabricated.

## Decision

1. Add `GraphAtKnownBy(generation, accepted_by)`: return the historical graph
   for the exact modeled generation only when every accepted transition from
   the retained history anchor through that generation has known acceptance
   time no later than `accepted_by`.
2. Persist `accepted_through_unix_nanos` per generation as the maximum
   acceptance time across its full parent chain. If any generation in that
   chain has unknown acceptance time, the watermark remains unknown for that
   generation and all descendants. Compute it atomically with accepted graph
   publication; backfill it transactionally during SQLite/Turso migration.
3. Read the watermark by exact generation key, then reuse the existing
   generation-root snapshot query. Reject an unknown watermark as indeterminate
   and reject a watermark later than the requested cutoff. Do not use producer
   observation time or StateChronicle verification time as acceptance time.
4. Export the query through graph NDJSON schema v4. Its header includes both
   the modeled generation and the requested acceptance cutoff, plus the
   accepted-through watermark. The record mode remains
   `historical_conclusion`; this is not retrospective re-evaluation under newer
   semantics.
5. Add `graph-at-known-by[-turso] <store> <generation> <accepted-by-ns>` to the
   local CLI. Reference stores may compute watermarks from retained history;
   durable stores must perform indexed, history-depth-independent point reads.

## Consequences

The API answers one precise two-axis question without delta replay: was this
exact modeled generation fully part of SyntaxMesh's known state by the
requested acceptance cutoff? Unknown legacy history is reported as
indeterminate. This does not select state by producer observation time, infer
calendar validity, or apply current semantics to historical facts.
