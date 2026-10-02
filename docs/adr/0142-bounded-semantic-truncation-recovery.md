# ADR-0142: Recover truncated multi-document semantic prompts automatically

- Status: accepted
- Date: 2026-09-30

## Context

Graphify's direct inference adapter packs documents into parallel requests and
bisects batches when the provider explicitly reports truncated output.
SyntaxMesh already packs independently cached document requests, but currently
fails the entire semantic step on `finish_reason=length`.

## Decision

Keep `syntaxmesh index --semantic` as the single opt-in entry point. In the Rust
CLI adapter, classify explicit output truncation separately from other errors.
Bisect a truncated multi-request prompt deterministically, to at most three
split levels (at most fifteen prompt executions for one original group).
Merge successful subprompt output before the existing exact-evidence and
independent-request ownership validation. Singleton requests and exhausted
split budgets fail explicitly; do not truncate source text or invent claims.

Do not retry malformed JSON, invalid evidence, authentication failures, or model
revision changes through this mechanism. Existing bounded HTTP transient retries
remain separate; their attempts multiply the prompt-execution bound. Child
prompts execute sequentially inside a worker, retaining the four-worker limit.
Normal successful prompts pay no extra inference calls. Token accounting counts
all responses, including truncated ones.

Nullable completion content is decoded before classifying the finish signal,
following the nullable-provider-content handling in Sim's provider adapters.
`length` with null content still enters bounded recovery; null content without
that signal fails explicitly rather than being accepted as an empty extraction.

No new public DTO, engine dependency, cache schema, provider SDK, runtime, or
command is introduced. Document cache identities remain independent of prompt
packing, and semantic graph publication remains atomic. Child results are not
separately journaled: if a sibling fails, that original group's documents remain
retryable rather than being partially published.
That initial child-cache limitation is superseded by
[ADR-0143](0143-cache-successful-semantic-recovery-leaves.md): successful,
validated leaves now complete existing per-document Penelope records while the
overall semantic graph still publishes atomically.

## Limits

This improves recovery, not cross-document synthesis or demonstrated model
quality. A claim still must be supported within one independently cached request.
Token-aware provider-specific input budgets and recovery inside a singleton
request remain future work. No faster-than-Graphify benchmark is claimed.

## Verification

Unit tests cover single-call success, deterministic splits, singleton failures,
depth exhaustion, and preserving independent evidence for equal assertions.
CLI fixtures on File and verified Turso serve a truncated two-document response
followed by two valid single-document responses. With the provider subsequently
closed, both documents reuse cached results and retain the same generation.
Both backends also cover truncation with null content. This regression failed
before nullable wire decoding: the CLI rejected the response JSON before it
could inspect `finish_reason`.

## References

- [Graphify inference implementation](https://github.com/Graphify-Labs/graphify/blob/v8/graphify/llm.py)
- [ADR-0119](0119-one-command-parallel-semantic-indexing.md)
- [ADR-0127](0127-explicit-semantic-prompt-request-boundaries.md)
