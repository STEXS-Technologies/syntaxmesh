# ADR 0272: Explicit context candidate truncation warnings

Status: accepted, 2026-10-01

Follow-up decision: also emit `lexical_query_terms_truncated` when more than
16 distinct normalized query terms are supplied. The existing selector searches
only the first 16 terms in lexical order, although scoring uses the complete
term set. Preserve this behavior pending a reviewed retrieval policy, but make
the omitted lookup work explicit for current and historical requests. Exactly
16 terms must not produce this warning. No DTO, schema or ranking change.
The existing context fixture checks 16 terms, 17 terms, and a duplicate-term
query: the warning appears exactly once only for 17 distinct terms. All nine
context unit tests pass after this follow-up.

Whole-Sim and whole-Shardline explicit-seed probes retrieve all known targets,
while natural-language selection still misses several. The existing context
compiler caps each term lookup before ranking, then caps the merged seed set;
these separate limits currently have no dedicated warning.

Keep scoring, lookup limits, final candidate limits and canonical search unchanged.
Emit `lexical_lookup_limit_reached` once when any per-term response fills its
lookup cap. This means completeness is unknown, not proven extra matches: the
search port has no continuation flag. Emit `lexical_candidates_truncated` when
the merged seed set demonstrably exceeds the requested candidate limit.
Warnings use the existing bounded ContextWarning DTO and do not disclose source
contents or expand candidates. This is selection observability, not a retrieval
fix, performance bound or alternative release acceptance criterion.

Implemented in the shared current/historical context seed selector. The existing
source-evidence fixture now checks merged truncation without lookup saturation,
then adds matching nodes in a second generation and checks each warning appears
once when both limits apply. All 36 query tests passed before the saturated-stage
addition; the expanded targeted fixture and strict query Clippy pass afterward.
Warning text consumes the existing serialized token budget; broader context
conformance and retrieval evaluation remain required.

The shared context-pack backend/restart conformance scenario passes. Whole-Sim
bundle `context-20261001T145543.794807Z-uncommitted` preserves the same 3/10
natural-language target hits across 2,048/8,192 budgets; TypeScript/Python/Bash
still miss at the required larger budget. All five explicit target-seed probes
still return source evidence within generation/token bounds. Warnings therefore
provide observability without establishing a retrieval fix or relaxing acceptance.
The complete `cargo make ci` gate passes in 225.95 seconds, including strict
workspace Clippy, dependency policy, workspace/doc tests, all 16 backend
conformance scenarios and API documentation.
