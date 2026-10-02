# ADR-0130: Report provider token usage in the CLI host

- Status: accepted
- Date: 2026-09-30

## Context

The source-of-truth semantic roadmap requires usage/cost accounting where a
provider supplies it. Graphify aggregates provider token reports during parallel
extraction. SyntaxMesh's adapter currently discards compatible response `usage`
fields, so operators cannot measure inference consumption or distinguish absent
usage from zero consumption.

## Decision

1. Aggregate reported `prompt_tokens`, `completion_tokens`, and `total_tokens`
   per CLI invocation. Track valid, missing, and malformed reports separately.
   Counts represent provider reports, not tokenizer estimates or monetary cost.
2. Use thread-safe host counters across packed parallel inference calls. Check
   token-total overflow and report unknown totals rather than wrapping values.
3. Record reports before semantic evidence/revision validation, so rejected
   completions still contribute observed usage. Attempts without usable reports
   remain explicit. Malformed optional usage must not reject otherwise valid
   inference output. Metadata requests are excluded from inference token usage.
4. Emit a separate usage summary after semantic enrichment succeeds or fails.
   Cache hits add no inference usage. Keep credentials and provider response
   bodies out of this output.
5. Keep counters in the Rust CLI host; do not change semantic DTOs, Penelope
   cache/journal formats, core, store schemas, or NDJSON exports. Durable usage
   history, provider-specific pricing, and monetary accounting remain open.

## Verification

Host tests cover concurrent aggregation, absent/malformed reports, and checked
overflow. The CLI revision fixture covers provider reports, cache hits, and
usage from a completion whose semantic publication is rejected. `cargo make ci`
passed on 2026-09-30, including strict Clippy and architecture boundaries.

## References

- [Graphify provider extraction and usage aggregation](https://github.com/Graphify-Labs/graphify/blob/v8/graphify/llm.py)
- [SyntaxMesh semantic subsystem](../syntaxmesh-todo-extensible-opensource-v5.md#28-optional-semanticllm-subsystem)
- [ADR-0119: One-command semantic indexing](0119-one-command-parallel-semantic-indexing.md)
