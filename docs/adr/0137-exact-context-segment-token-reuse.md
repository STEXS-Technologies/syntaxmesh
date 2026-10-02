# ADR-0137: Exact context segment token reuse

- Status: accepted
- Date: 2026-09-30

Extend ADR-0136's verified numeric-boundary rule to canonical ContextPack JSON
integer fields `token_budget`, `token_count`, item `rank`, `line_start`, and
`line_end`. Each numeric run is bounded by a colon and comma/closing brace.
The pinned encodings match these digits independently; the adjacent nonnumeric
segments end/start in punctuation, not whitespace, so end-of-input whitespace
rules cannot change segmentation. Encode each segment using the upstream
ordinary encoder and reuse only byte-identical segments across trials.

Require typed parsing and canonical reserialization. Retain at most 256 KiB of
segment-key text and 1,024 entries per request. Unknown/noncanonical/oversized
layouts use full encoding. This is exact segmentation at proven tokenizer
boundaries, not sums of independently tokenized arbitrary items. Keep the
tokenizer version pinned; other encodings require separate proof.

Differential tests compare full encoding for both supported encodings, Unicode,
escaped marker-like text, growing/shrinking item lists, numeric widths, and
fallbacks. Run the existing profiled benchmark and contributor gate; no public
DTO, pure-crate dependency, or packing policy changes.

## Verification evidence

Both pinned encodings match full ordinary encoding on growing/shrinking source
item lists, Unicode/escaped text, null and maximal spans, integer digit-width
boundaries, invalid/noncanonical inputs, and oversized fallback. Retained key
bytes and entry counts are checked against their bounds.

Three fresh Turso Rust-fixture runs without concurrent CI retained 9/9 targets
at both budgets and unchanged packed-token/omission ranges. Against ADR-0136's
immediately preceding fixed-point memo fixture, median query times changed from
487,118 to 226,748 µs at 2,048 tokens and 1,359,044 to 458,709 µs at 8,192 tokens.
Evidence: ignored `target/context-retrieval-results/context-20260929T220838.763058Z-uncommitted/`.
These are directional single-host fixture results, not broad repository claims.

`cargo make ci` passed, including strict Clippy, architecture checks, backend
context conformance, MCP stdio integration, and documentation builds.

Cross-language follow-up repeats the existing selected-file runner three times
per repository without concurrent CI. Sim and Shardline each retained 15/15
targets at both budgets. Comparing all 60 per-query rows against ADR-0136's
preceding sibling runs after removing only `query_us` found identical target,
retrieval, token, item, omission, and selected-input-fingerprint data.

Current median query times (2,048 / 8,192 tokens): Sim 462,035 / 1,066,388 µs
versus 982,310 / 3,147,756 µs previously; Shardline 294,881 / 766,521 µs versus
777,149 / 2,716,997 µs previously. This is roughly 53–72% lower in these selected
fixtures, not a whole-repository performance guarantee. Sim covers TS/JS/Python/
Bash/Markdown; Shardline covers Rust/Python/Bash plus a heading and authored
rationale paragraph. Raw evidence bundles:
`target/context-retrieval-results/context-20260929T221042.079201Z-uncommitted/`
and `target/context-retrieval-results/context-20260929T221109.088290Z-uncommitted/`.
Selected-file input hashes are unchanged from the baseline bundles recorded in
ADR-0136. Broader relevance/omission evaluation remains open.
