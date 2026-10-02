# ADR-0031: Keep modeled validity, observation, and knowledge time distinct

- Status: accepted
- Date: 2026-09-27

## Context

The source of truth requires bitemporal-style fact semantics and explicitly
distinguishes when a fact was true from when SyntaxMesh learned it (§235). It
also requires historical conclusion mode to remain distinct from retrospective
evaluation under current semantics (§238). The current implementation exposes
generation validity intervals for node payloads, and runtime observations carry
a producer-supplied `observed_at_unix_nanos`. It has no first-class acceptance
time and no calendar-time query contract. SQLite and Turso currently extract
the runtime observation timestamp into the temporal fact-version row for
runtime-observation nodes, but this metadata has no public indexed query and
does not cover other fact families. Treating the generation ID or its sequence
as a timestamp would produce incorrect delayed-observation and incident
answers.

## Decision

Represent three separate axes; do not collapse them into one `timestamp`:

1. **Modeled validity:** the half-open generation interval in which a canonical
   graph fact is present in accepted SyntaxMesh state. Generation IDs identify
   states; they are not dates. Calendar/domain validity intervals, when a fact
   family needs them, are additional typed evidence and must not be inferred
   from commit time.
2. **Observation time:** the producer-reported time an event/evidence item
   occurred or was observed. Preserve it with its producer and provenance. It
   is an untrusted assertion, may be absent for non-temporal facts, and must not
   be rewritten to the import time. Runtime protocol v1 already carries this
   value for runtime observations only.
3. **Knowledge/acceptance time:** when the engine durably accepted a generation
   containing the fact version. Record this independently of fact payload,
   generation identity, observation time, and StateChronicle verification.
   Assign it at publication through a runtime-neutral injected clock, persist it
   atomically with the generation, and preserve it during replay/migration. The
   timestamp must be captured in the durable Penelope operation before commit,
   so recovery reuses the original value rather than assigning a new one. It is
   metadata and does not enter stable fact IDs or the canonical graph root.

Generation intervals remain the v0 modeled-validity axis. A future bitemporal
query must accept both a modeled generation/time and a knowledge cutoff, and
must intersect the fact's validity with the generations accepted by that
cutoff. A calendar-time query over producer observations must use the explicit
observation metadata/index, not reinterpret generation time. Until those
indexes and query DTOs exist, expose no API named `StateAtTime` or `KnownAt` and
do not claim full bitemporal querying.

Historical conclusion mode answers from the graph, producer/configuration
fingerprints, and evidence actually accepted for the selected generation.
Retrospective-current-semantics mode combines old source/system state with
newer semantic capabilities; it is a separate future query mode and must be
labelled as such. Existing generation queries are historical conclusion mode.

Implementation contract: define a typed acceptance-time value and injected
clock in the runtime-neutral engine boundary; persist acceptance metadata
atomically with each generation without changing graph roots; preserve
`unknown` for legacy generations/operations; add versioned query/export DTOs
only with cross-backend and serialization fixtures. Never backfill unknown
timestamps from file modification times or invent producer observation times.
Recovery tests must prove a prepared operation and its retried generation keep
the same accepted-at value across a crash on either side of graph commit.

The typed value, injectable clock, and atomic generation metadata are now
implemented. SQLite v6→v7 and Turso v10→v11 create the acceptance table and
ordered timestamp index; File/InMemory persist known values in their existing
durable-record namespace, leaving legacy values absent/unknown. Penelope record
v2 captures the time before graph commit; its decoder continues to accept v1
records as unknown. Restart and crash-recovery fixtures verify captured times
are preserved. The bounded acceptance timeline is defined by [ADR-0032](0032-indexed-acceptance-timeline-query.md), and producer-observation
pages by [ADR-0034](0034-indexed-observation-timeline.md). They list timeline
evidence but do not select/reconstruct graph state at calendar time.
Bitemporal state selection remains unimplemented; do not claim full
calendar-time or bitemporal query support yet.

The observation index and bounded range query are implemented for runtime
observations. SQLite v5→v6 and Turso v9→v10 convert timestamps from bincode's
little-endian bytes to fixed-width big-endian bytes; SQLite v7→v8 and Turso
v11→v12 extend the partial index to
`(observed_at, fact_kind, fact_id, valid_from_sequence)` for deterministic
cursor continuation. Migrations fail closed on malformed legacy values. This
covers runtime observations only; other fact families need explicit producer
time semantics and metadata extraction.

## Alternatives considered

- **Use generation sequence as time:** rejected because imported/delayed evidence
  and acceptance time do not equal generation order or elapsed calendar time.
- **Overwrite observation time with import time:** rejected because it destroys
  the event time needed for retrospective analysis.
- **Add one timestamp to every fact payload:** rejected because it conflates
  producer claims with engine acceptance, changes fact identity/root behavior,
  and makes static facts pretend to have event times.
- **Implement a generic bitemporal database now:** rejected for v0; the logical
  contract needs all three axes, but each fact family should add domain-valid
  intervals only when their meaning and provenance are defined.

## Consequences

- ADR-0026/0027/0028 remain accurate only as generation-time query contracts;
  their notes about missing acceptance/calendar semantics are superseded by
  this decision, not by unimplemented behavior.
- Acceptance time is transaction metadata, while producer observation time is
  evidence metadata. StateChronicle verification time remains a third concern.
- Durable observation timestamps are indexable without scan or value
  ambiguity. ADR-0033 exposes them with canonical fact history and ADR-0034
  adds bounded indexed observation-time range queries.
- Acceptance timestamps now round-trip for newly published generations and
  remain unknown for legacy history. The bounded acceptance timeline can be
queried by indexed time range; by itself this does not provide as-of graph
state. ADR-0034 closes the initial observation-time range-query gap. ADR-0047
adds exact-generation state qualification by acceptance cutoff using the
ancestry-prefix watermark; this is not generic calendar-valid-time
reconstruction. Initial indexed change/consequence history is implemented by
ADRs 0036, 0041–0045. The remaining history gate includes domain-defined
calendar-valid-time semantics, broader scale/storage evidence, and future
transitive analysis. Canonical fact histories are available under ADR-0033. A
generation-only `GraphAt` remains valid and history-depth-independent.
