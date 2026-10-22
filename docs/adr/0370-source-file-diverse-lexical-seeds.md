# ADR-0370: Source-File-Diverse Lexical Seeds

Status: accepted

Date: 2026-10-22

## Context

Whole-repository source-content retrieval can index the intended evidence and
still omit it from the seed set. The current planner ranks candidates, filters
by fact family, and takes the first 32 from each family/channel before
interleaving those queues into a 32-node seed set. Repositories with many
matching spans in one file can therefore crowd out otherwise relevant files;
the capped set prevents later graph expansion from recovering them. Recent
Penelope repository evaluation missed two of three targets at both tested token
budgets, while diagnostics showed relevant source evidence competing within the
same file-heavy candidate pool.

## Decision

1. For the explicitly opt-in source-span-augmented planner only, preserve the
   exact/stemmed scoring function, family/channel ordering, aggregate posting
   work budget, 32-seed ceiling, and graph/context limits.
2. When forming each family/channel candidate queue, scan its complete already
   ranked result and prefer the highest-ranked candidate from each distinct
   source file before allowing a second candidate from a file. Preserve score
   order among file leaders, then fill any remaining queue capacity with the
   existing score order. Candidates without source provenance remain distinct
   singleton groups.
3. Keep label-only planner behavior byte-for-byte ordered as before. Do not add
   target-specific quotas, source-language heuristics, public DTO fields, or a
   new runtime/host dependency.
4. Charge retained source-file ownership metadata to the source index payload
   budget. Keep the source-content planner opt-in until repeatable multi-corpus
   quality and build-cost evidence supports a policy decision.

## Consequences

Source-content seed selection can reach relevant files whose individual nodes
rank below 32 because one file contributes many stronger-scoring matches. The
same total seed and graph limits remain in force. Per-file diversity can lower
the rank of a second useful item from one file; the source-content evaluation
must measure both target recall and packed evidence quality. Historical indexes
and label-only seed order are unchanged.

## Verification

Tests must prove a lower-ranked file leader survives without changing the seed
ceiling, ordinary planner order remains unchanged, complete lexical scores still
match the reference oracle, aggregate posting budgets remain enforced, and
source-file metadata exhaustion fails index construction. Run matched
repository-scope retrieval on approved STEXS corpora at both 2,048 and 8,192
tokens. A single improved corpus is iteration evidence, not default-promotion
acceptance.

## References

- [ADR-0368](0368-bounded-source-span-lexical-planner.md)
- [ADR-0369](0369-budgeted-source-evidence-excerpts.md)
- [v0 architecture and implementation plan](../V0_ARCHITECTURE_PLAN.md)
