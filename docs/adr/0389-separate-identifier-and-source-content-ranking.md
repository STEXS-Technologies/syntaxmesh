# ADR-0389: Separate identifier and source-content ranking signals

Status: accepted

Date: 2026-10-22

## Context

The opt-in source-content index inserted body terms into the same exact
postings as node labels. As a result, broad matches in a large function body
could outrank a concise function whose identifier directly matched the
question. On the selected Shardline fixture, `load_metadata` was found at
identifier rank 9, selected, and still missed at both token budgets. Its span
was 435 bytes; preceding selected body candidates included a 19,854-byte Rust
function. An enriched opt-in diagnostic now records each seed's label score,
rank, source span size, and pack position.

The prior source-content policy retrieved 6/7 selected Shardline targets at
both budgets on input fingerprint
`1975e5e80026ee17608ed1e7fb893e9ec90c00831c6683684fce47e798e51139`. This
fingerprint is from a dirty external checkout; the comparison is scoped to
those exact selected inputs and is not repository-wide acceptance.

## Decision

Keep node-label and source-body exact postings as separate bounded indexes.
Within opt-in source-content seed scoring, add the exact label score to the
stronger of the body exact and body stemmed scores; use label score as the
deterministic tie-breaker, then stable node ID. Keep label-only query results
unchanged. Preserve the same aggregate posting-work budget, independent source
index limits, primary-family precedence, distinct-file opportunity, 32-seed
ceiling, and opt-in status. Use this score order for original candidates when
composing the final packing plan so the evidence pack does not discard the
score-aware seed order.

## Alternatives

- Keep exact labels and body terms merged: rejected because the planner cannot
  distinguish an exact symbol-name match from broad prose matches.
- Rank solely by body score or source-span size: rejected because it can bury a
  directly named symbol or overfavor short but weak evidence.
- Raise candidate/token limits or add language-specific quotas: rejected as
  higher-cost and brittle responses to a ranking-signal problem.

## Consequences

The source-content planner retains its bounded work and output limits but uses
additional separately charged posting-key storage. Exact source-content query
results still see the union of label and body postings without double-counting
the same node-term posting. The opt-in plan may reorder original candidates;
label-only planning and persisted graph state are unchanged.

## Verification

Planner tests cover label/body score ties, exact/stem fusion, family ordering,
file diversity, aggregate work, and the seed ceiling. Three fresh selected
Shardline runs on the same input fingerprint retrieved 7/7 targets at both
2,048 and 8,192 tokens (21/21 checks per budget); median query latencies were
1.71 s and 3.65 s. A one-run prior-code comparison on the identical selected
input missed `load_metadata` at both budgets (6/7). Penelope remains 12/12 and
StateChronicle 9/9 at each budget in three-run checks. These fixed-target
samples do not establish whole-repository quality, general latency, or
promotion of source-content planning to the default. No public DTO, persisted
format, migration, or host contract changes.
