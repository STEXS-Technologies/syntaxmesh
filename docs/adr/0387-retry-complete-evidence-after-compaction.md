# ADR-0387: Retry complete evidence after compaction

Status: accepted

Date: 2026-10-22

## Context

The context packer first tries each selected source item in full. If that fails,
it generates bounded excerpts only for source spans larger than 512 bytes. The
small-span fast path returned before attempting to compact already-admitted,
lower-priority evidence. In addition, after compacting an earlier item, the
long-span path tried only candidate excerpts, even when the candidate's complete
source would now fit.

A fresh selected-file Shardline retrieval sample reproduced the issue for the
Python `load_metadata` target: it was selected by the planner but absent at both
2,048 and 8,192 tokens. Its source span is 710 bytes, so it exercises the
long-span path: after compacting earlier items, that path tried excerpts but
never retried the complete function. Other targets and the 2,048-token
documentation rationale target also missed in that seven-case sample. The
sample identifies a packing-path defect; it is not a broad quality or latency
claim. Raw evidence is retained under ignored
`target/context-retrieval-results/context-20261022T181248.492808Z-7d13989b83f3/`.

## Decision

After a complete candidate fails in the current pack, allow the existing
bounded, reversible source-compaction path to create room and retry the
candidate's complete item before trying its excerpts. This also applies when
the candidate is too short to warrant excerpt generation or exceeds the
remaining excerpt-scan budget. If compaction cannot make the complete candidate
fit, preserve existing excerpt fallback for eligible long spans and otherwise
record the omission as before.

Keep selected-candidate ordering, graph selection, token counting, source
identity, output bounds, and ContextPack schema unchanged. Continue to restore
the prior pack if all admission attempts fail or token counting errors.

To bound additional token-count work, attempt at most six compacted pack states
for complete-candidate retry. Preserve the first fitting excerpt pack as a
fallback while checking later compacted states for a complete-candidate fit;
prefer complete evidence when found, otherwise restore that excerpt pack.

## Alternatives

- Increase the selected-node or token limit: rejected because it hides the
  missed retry and increases routine work.
- Add a short-candidate or language-specific quota: rejected because candidate
  admission is already bounded and must remain runtime/language agnostic.
- Retry only an excerpt: rejected because the complete source may fit after
  lower-priority evidence is compacted, and completeness is preferred.

## Verification

Add a deterministic test where the complete short candidate fails against the
current pack, then fits after compacting an earlier multi-line source item.
Assert complete candidate text and identity are retained, earlier evidence is
explicitly clipped, ordering is unchanged, and exact token count remains within
budget. Retain the existing excerpt, rollback, and tokenizer-failure tests.
The four focused packing tests, Query strict Clippy, and formatting pass. A
one-run selected-file Shardline sample retrieved all seven targets at both
2,048 and 8,192 tokens after the change (versus the immediately preceding
cap-four sample's six of seven at each budget); median query latency was 1.09
and 2.19 seconds. This is a single dirty-fixture iteration sample, not a broad
performance claim.

This changes internal packing behavior only; it does not change a public DTO,
query option, default selection policy, or persisted representation.
