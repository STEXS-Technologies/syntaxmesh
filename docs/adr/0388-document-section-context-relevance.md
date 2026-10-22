# ADR-0388: Inherit document-section relevance from selected content

Status: accepted

Date: 2026-10-22

## Context

The selected-file Penelope context matrix repeatedly retrieves the README
`DocumentChunk` that matches a question about checkpoint, execution, fencing,
and recovery, but omits the containing `Section` heading at 2,048 tokens. The
heading is selected (seed position 14, packing position 19), so this is not a
candidate-discovery failure. Its lexical heading text has a lower score than
the matching prose and the heading loses when evidence is ordered for the
small token budget. The 8,192-token case retrieves it.

Three fresh indexed source-content runs on the same selected-file fingerprint
retrieved 9/12 target-budget checks (3/4 at 2,048 and 4/4 at 8,192). The
repeated miss is a structural-documentation ranking issue, not a request to
raise token or candidate limits. Evidence is retained under ignored
`target/context-retrieval-results/context-20261022T183114.770225Z-078bf55313cd/`.

## Decision

When a selected `Section` directly contains a selected `DocumentChunk`, give
the section candidate at least the relevance of that chunk for this context
pack. This keeps an authored heading near the prose that makes it relevant and
applies only to the selected graph evidence; it does not alter graph selection,
lexical seed scores, stored facts, or evidence text. Other node kinds and
unrelated sections retain their existing relevance.

Some indexed packing plans have already selected both nodes but intentionally
perform zero-hop expansion. In that case, read the selected section's direct
adjacency only to find `Contains` edges to selected chunks. Do not add those
edges to the pack or otherwise turn ranking support into graph expansion.

## Alternatives

- Increase token or candidate limits: rejected because this increases work and
  does not correct the mismatch between relevant prose and its heading.
- Add a document-specific quota or reorder all headings first: rejected as
  brittle and unrelated to query relevance.
- Change seed discovery or source-content scoring: rejected because both the
  heading and matching body chunk are already selected.

## Verification

Add a deterministic query-layer test proving that a selected section inherits
relevance only from its directly contained selected document chunks, without
changing other nodes. Three fresh indexed source-content runs on the selected
Penelope fixture now retrieve all four targets at both budgets (12/12 per
budget); median query latency is 0.89 s at 2,048 and 1.24 s at 8,192 tokens.
The selected StateChronicle fixture also retrieves all three targets at both
budgets (9/9 per budget), with medians 1.58 s and 3.17 s. The selected Shardline
fixture remains 18/21 at each budget: the Python `load_metadata` target is
missed in all three runs at both budgets. These results are fixed-target
iteration evidence, not corpus-wide acceptance or an SLA; exact bundles are
recorded in the implementation plan. No public DTO, persisted format,
migration, or host contract changes.
