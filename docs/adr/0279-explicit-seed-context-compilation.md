# ADR 0279: Explicit-seed context compilation preserves query intent

Status: accepted, 2026-10-02

Candidate-policy experiments currently compile with an empty query to avoid
merging the legacy lexical lookup into their planned seeds. That loses the
original question from context metadata and exact token accounting. Reuse the
existing selection, graph expansion, verified source access and exact packer,
not a separate retrieval or context service.

Add explicit-seed-only Query compilation and source-free selection methods for
current and retained generations. They accept the existing ContextRequest and
preserve its query verbatim, but perform no lexical lookup. Require at least
one seed; reject distinct seeds exceeding max_candidates rather than dropping
caller choices. Retain the existing 32-input-seed, hop, candidate and query-size
limits, canonical seed validation, deterministic ordering and source checks.
Ordinary context methods retain their existing lexical-plus-explicit behavior.

This is an additive composition boundary for independently planned seeds,
not acceptance of the rejected dual-channel definition-seed policy, proof of
relevance, or a default production-selector change. No DTO/wire-schema or storage
change, runtime dependency, model API or NDJSON handoff is introduced.

Verification: the shared Query context fixture now checks preserved question and
exact serialized token count, duplicate seed deduplication, lexical bypass,
one-hop expansion, deterministic current/historical equivalence, empty and
over-budget seeds, the 33-input bound, and unknown seed rejection. All 39 Query
and 15 non-ignored MCP unit tests pass; combined all-target/all-feature strict
Clippy passes. The ignored sibling retrieval diagnostic now uses this boundary
and preserves its question. Its earlier empty-question measurements remain
historical evidence, not measurements of this updated probe. No production
candidate policy or retrieval-quality claim changes.

The retained-context backend fixture also exercises historical explicit-only
compilation across InMemory/File/SQLite/Turso, including edits and durable
restart; that scenario passes. Full contributor CI after ADRs 0279–0280 passes
in 215.08 seconds, including all 16 backend scenarios. Concurrent whole-Sim
retrieval is still pending; the CI duration is not isolated performance evidence.
