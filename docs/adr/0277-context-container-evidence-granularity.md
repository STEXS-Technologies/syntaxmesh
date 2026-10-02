# ADR 0277: Local evidence before tied whole-source containers

Status: accepted, 2026-10-02

The test-only exact/stemmed definition-seed comparison in whole-Sim bundle
`context-20261001T202939.928911Z-uncommitted` retrieves Python/Bash/JavaScript/
Quickstart evidence within the unchanged 64-node/8,192-token bounds. TypeScript
is selected at depth 1 but absent from the pack. Its admitted items include a
10,144-byte, 200-line README container alongside its separately selected chunks.
Neither broad container evidence nor stable-ID order guarantees useful packing.

Extend ADR-0276's final source-granularity tie-break: local definitions and
documentation content precede reference occurrences, which precede whole-source
File/Module/Script/Document containers. Relevance and graph distance still win
before granularity. Explicitly selected containers retain their seed priority.
Reuse canonical node kinds and the shared current/historical exact packer;
do not trim source bytes or treat declaration spans as complete function bodies.

This is not acceptance of the full-scan dual-channel seed diagnostic as a
production policy. Production candidate selection remains unchanged. Focused
classification/ordering tests, corpus reruns and backend equivalence are required.

Whole-Sim rerun `context-20261001T203802.506431Z-uncommitted` returns source
evidence for all five diagnostic targets with unchanged 64-node/8,192-token
bounds. Production natural-language retrieval remains 3/10: neither full-scan
candidate scoring nor the dual-channel seed policy is enabled in production.
All 39 Query tests, separate strict Query/MCP Clippy and the three current/
historical backend context scenarios pass. Independent corpus verification and
answer-bearing body coverage remain open. A combined feature check separately
exposed the temporal-page instrumentation construction issue covered by ADR-0278.

Full contributor CI passes in 282.67 seconds, including all 16 backend conformance
scenarios and the new mixed-feature checks. Independent whole-Shardline bundle
`context-20261001T204709.088969Z-uncommitted` preserves the production baseline
6/10 target/budget hits and both documentation targets. The diagnostic seed policy
itself retrieves only 2/5 targets (losing Rust webhook selection compared with
all-fact exact seeds); reject that policy for production despite Sim's 5/5.
This distinguishes the verified packing tie-break from an inadequate selector.
CI and corpus evaluation ran concurrently; no isolated timing claim follows.
