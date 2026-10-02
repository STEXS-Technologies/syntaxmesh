# ADR-0135: Reversible exact context admission

- Status: accepted
- Date: 2026-09-30

Replace whole-`ContextPack` cloning for each candidate with a reversible
append/count/admit operation. Record the previous item length, omission summary,
and exact token count; restore them on rejection or counting failure. Reuse the
existing exact full-JSON fixed-point counter, ranking, complete-item policy,
and omission computation. No additive token approximation or new public
contract is introduced.

The clone-based path remains a differential test oracle across multiple budgets
and accepted/rejected Unicode/source items. A tokenizer-failure test checks
complete rollback. This removes redundant accepted-text copies, but does not
claim fewer tokenizer calls or a measured whole-query speedup.

Verified on 2026-09-30: all 22 query tests and `cargo make ci` passed.
The strengthened post-count-update failure fixture was additionally rerun
successfully with strict query-crate Clippy after the workspace gate.

The existing Shardline-style retrieval runner was then executed without concurrent
CI: three fresh Turso samples returned 9/9 fixed targets at both 2,048 and 8,192
tokens. Median query times were 780,801 µs and 2,226,950 µs. Packed token ranges
were 1,953–2,048 and 8,103–8,140; omissions remained explicit. Evidence is retained
under ignored `target/context-retrieval-results/context-20260929T214935.862768Z-uncommitted/`
(source fingerprint `83ac3f4b1d7739419e39992e96c27e018611bbc2ed305fb4f9f5b8af21cddb65`).
The previous run overlapped CI and is not an isolated control. This verifies
fixed-target retrieval after the allocation change, not a speedup or broad
repository relevance. Multi-second larger-budget latency remains open and needs
stage-level profiling before selecting the next optimization.
