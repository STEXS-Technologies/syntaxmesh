# ADR-0369: Budgeted Source-Evidence Excerpts

Status: accepted

Date: 2026-10-22

## Context

ADR-0368 improves whole-repository candidate recall when relevant terms appear
inside source spans. The StateChronicle full-repository evaluation confirms a
remaining small-budget failure after the intended function is selected: its
complete source span ranks after several larger source items and cannot fit in
the remaining 2,048-token pack. Raising graph limits or assigning target quotas
would hide this packing failure rather than solve it generally.

## Decision

1. Try the complete source-evidence candidate first and unchanged. If it does
   not fit, try compact windows for that same selected node. If those still do
   not fit, compact already-admitted source evidence from lowest to highest
   priority, only as needed to admit the next selected candidate. Preserve item
   order and identity; restore all prior items if admission still fails.
2. Derive a bounded set of contiguous, whole-line windows from the already
   validated source span using a fixed-size line ring, scanning at most 8 MiB of
   candidate spans and retaining at most 256 KiB of excerpt text per context
   compilation. Each window is at most 16 KiB. Prefer windows with the greatest
   coverage of the query's normalized terms; try successively smaller maximum
   widths of 24, 12, 6, 3, and 1 lines. Use deterministic earliest-window
   tie-breaking.
3. Mark clipped content explicitly with a neutral `[…]` line. Keep the same
   node identity, evidence class, source path, and exact one-based line bounds;
   never imply that an excerpt is the full declaration or document chunk.
4. Preserve candidate ordering, graph traversal, tokenizer authority, and
   omission accounting. Complete evidence is retained unless budget pressure
   requires compaction to admit another candidate. Count one source-evidence
   candidate regardless of how many fallback windows are tried.
5. Do not change the ContextPack schema, add host/runtime dependencies, read
   source outside the existing validated provider, or add per-target quotas.

## Consequences

Relevant selected evidence can survive constrained budgets by compacting spans
without changing their identities or relative order. Fallback generation has
independent scan and output bounds; packing attempts compaction only after exact
full-item rejection. Excerpt text is explicitly partial and remains tied to the
original source lines and node. Default retrieval may include excerpts where it
previously omitted selected source items; ordering and graph selection remain
unchanged.

## Verification

Tests must prove complete-first admission, deterministic term-centered fallback,
correct clipped line coordinates and markers, one-candidate omission accounting,
exact tokenizer-budget compliance, and unchanged behavior when complete source
fits. The whole-repository StateChronicle comparison must be rerun; the source
index remains opt-in until cost and quality gates pass across approved corpora.

## References

- [ADR-0368](0368-bounded-source-span-lexical-planner.md)
- [v0 architecture and implementation plan](../V0_ARCHITECTURE_PLAN.md)
