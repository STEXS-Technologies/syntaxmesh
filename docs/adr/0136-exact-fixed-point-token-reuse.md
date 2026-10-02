# ADR-0136: Exact fixed-point token reuse in the MCP host

- Status: accepted
- Date: 2026-09-30

For the two supported, pinned tiktoken encodings, ordinary encoding splits
ASCII numeric runs independently (`\p{N}{1,3}` in both upstream patterns).
Canonical ContextPack JSON places its unsigned `token_count` between a colon
and comma. Therefore changing only that integer changes only its independently
encoded digit tokens, not the rest of the payload.

Reuse the previously full-encoded remainder when canonical payloads differ
only in this integer. Require successful typed parsing and identical canonical
reserialization; compare the complete remaining bytes, not a hash or substring
estimate. Unknown layouts, noncanonical inputs, or payloads over 256 KiB use
ordinary full encoding. Retain one memo entry per request and reset it on MCP
context invocation. Keep tokenizer identity, fixed-point accounting, public
contracts, and exact greedy packing unchanged.

Validate against full encoding for both supported encodings, digit-width
boundaries, Unicode, escaped marker-like text, changed content, and fallback
inputs. Reuse the existing profiled evaluation runner before claiming latency
improvement. This optimization must not be reused for other encodings without
their independent-boundary proof and differential verification.
Pin the host tokenizer dependency exactly to 0.12.1 so an incidental patch
upgrade cannot change the regex assumptions without explicit revalidation.

Proof source: pinned `tiktoken-rs` 0.12.1 `tiktoken_ext/openai_public.rs` and
`CoreBPE::encode_ordinary`'s per-regex-match encoding loop.

## Measured fixture evidence

Three fresh Turso runs without concurrent CI retained 9/9 target hits at both
budgets. Against the immediately preceding profiled full-encoding run, median
query times changed from 777,859 to 487,118 µs at 2,048 tokens and 2,227,114 to
1,359,044 µs at 8,192 tokens (about 37–39% lower). Packed token and omitted-item
ranges were unchanged. This is one fixed Rust fixture on one host, not an SLA
or general repository performance claim. Baseline evidence:
`target/context-retrieval-results/context-20260929T215214.457821Z-uncommitted/`;
memo evidence: `target/context-retrieval-results/context-20260929T215827.723476Z-uncommitted/`.
Both paths are ignored local evidence bundles with raw logs and provenance.

`cargo make ci` passed with expanded differential cases covering source items
and pretty-JSON fallback, strict Clippy, architecture boundaries, backend
context conformance, stdio MCP integration, and documentation builds.

Cross-language follow-up: three fresh Turso runs on Sim revision `63f18995da8c`
(clean) retrieved 15/15 fixed TypeScript/Python/JavaScript/Bash/Markdown targets
at both budgets. Selected-input BLAKE3:
`e947fc9fbacdd075fc0a03cf57a5a6397d994b8f4991e524632f85bb4f6cebb0`.
Median context queries were 982,310 µs at 2,048 tokens and 3,147,756 µs at 8,192;
larger-budget latency remains material. Evidence:
`target/context-retrieval-results/context-20260929T220109.015317Z-uncommitted/`.
This is selected-file retrieval evidence, not full-Sim indexing, language
completeness, or corpus-wide relevance.

Shardline follow-up: three fresh runs on revision `45ec159a764d` (dirty) also
returned 15/15 fixed Rust/Python/Bash/Markdown targets at both budgets, including
an authored benchmark-rationale paragraph rather than just a document heading.
Selected-input BLAKE3:
`cb7574e941c3f41479b279dd9823fea985301dece616324d5b213e5111899124`.
Median queries were 777,149 µs and 2,716,997 µs. Evidence:
`target/context-retrieval-results/context-20260929T220239.360367Z-uncommitted/`.
The corpus stays selected-file and latency remains workload-dependent.
