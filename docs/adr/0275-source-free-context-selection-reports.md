# ADR 0275: Source-free context selection reports

Status: accepted, 2026-10-01

Whole-Sim seed-only pack diagnostics show no target items/warnings for
TypeScript/Python/Bash, although independent traversal reaches TypeScript.
Absent packed items do not distinguish selection from packing failure.

Expose an explicit query-layer report of selected node IDs, graph depths,
lexical relevance, edge count and existing warnings, pinned to one generation.
Current and historical methods reuse exactly the compiler's seed/expansion
functions; do not implement another traversal or emit source/name payloads.
The report is bounded by existing request limits and does not read source
files, count tokens or change default ranking. Its validity is selection only,
not evidence correctness, complete graph coverage or context-pack inclusion.

The existing context fixture compares current/historical reports, stable-ID
ordering, generation, selected count and compiler warnings. All 37 query tests
and strict Query Clippy pass. The opt-in whole-corpus evaluator now logs target
depth from this report alongside packed item kinds, without changing acceptance.

Whole-Sim `context-20261001T195918.793482Z-uncommitted` confirms the
TypeScript target is selected at depth 1 but has no packed item; Python/Bash
are not selected. JavaScript/Quickstart are selected at depth 0 and include
source evidence. This isolates a TypeScript packing failure from the other
candidate failures; source-span sizing/ordering needs inspection before a fix.
