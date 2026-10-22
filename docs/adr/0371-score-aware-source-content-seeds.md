# ADR-0371: Score-Aware Source-Content Seeds

Status: accepted

Date: 2026-10-22

## Context

ADR-0370 preserves eight separate family/channel queues for the opt-in
source-span planner. Their round-robin refill can leave a high-ranked candidate
out of the fixed 32-seed set. In the whole-repository Penelope evaluation,
`BlobRef::verify` ranks 24th in the exact-token candidate results but is absent
from the lexical seeds. The corresponding README-section target is also missed.
This is a candidate-allocation failure before source packing; the current
StateChronicle whole-repository run passes all three targets, while Penelope
does not.

## Decision

1. Change only source-span-augmented seed construction. Keep label-only and
   historical planner ordering unchanged.
2. Fuse exact and stemmed candidate scores per node using the higher score,
   avoiding double-counting a node present in both channels. Rank deterministically
   by score descending and stable node ID.
3. Consider code and documentation candidates before reference/other families.
   Select up to half the seed limit from the highest-ranked distinct source-file
   leaders, then fill remaining capacity in global score order from primary
   families. Use reference/other candidates only if primary families have spare
   capacity. Candidates with no source provenance remain singleton file groups.
4. Preserve the existing aggregate posting-work budget and hard seed limit.
   Keep source-content planning opt-in; no target-specific rules or public DTOs.

## Consequences

The top relevance-ranked source candidates are no longer partitioned into
fixed-size exact/stemmed queues before seed admission. Half of the bounded seed
set still gives distinct files a first-pass opportunity; the other half protects
strong same-file and cross-family candidates. Score fusion changes only the
opt-in source-content seed order and can alter which references receive spare
capacity. Multi-corpus quality and build/query costs must be measured before
promoting this policy.

## Verification

Add planner tests proving: a rank-24 primary candidate survives the seed cap;
a lower-ranked distinct-file candidate can still enter through the diversity
quota; exact/stem overlap is not duplicated or double-scored; non-primary
families fill only unused primary capacity; ordinary planner order remains
unchanged; and the aggregate posting budget and 32-seed ceiling remain enforced.
Rerun whole-repository retrieval on the approved STEXS Penelope and
StateChronicle corpora at 2,048 and 8,192 tokens. A single passing corpus is
iteration evidence, not default-promotion acceptance.

## References

- [ADR-0370](0370-source-file-diverse-lexical-seeds.md)
- [ADR-0368](0368-bounded-source-span-lexical-planner.md)
- [v0 architecture and implementation plan](../V0_ARCHITECTURE_PLAN.md)
