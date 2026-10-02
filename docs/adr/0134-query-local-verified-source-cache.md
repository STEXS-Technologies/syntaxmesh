# ADR-0134: Query-local verified source reuse

- Status: accepted
- Date: 2026-09-30

Context compilation currently rereads and rehashes a file for each selected
source-backed node. Reuse Shardline SDX's shared decoded-block pattern
(`crates/sdx/src/stream.rs`, `XorbBlock::data`) for verified source text within
one context compilation. Key entries by file ID and accepted content hash;
cache provider failures, unavailable content, hash mismatch, and invalid UTF-8
as well as successful text. Preserve per-node warnings and exact span checks.

Cap retained successful text at 8 MiB per query. Files exceeding remaining
capacity are checked normally but not retained. Drop all entries after the
query, so later queries observe source changes and retry provider failures.
No host/global cache, runtime dependency, public DTO, tokenizer approximation,
or durable format changes. Exact context packing remains unchanged.

Verify provider-read reuse, identity separation, query-local invalidation,
error behavior, bounded retention, and existing backend context conformance.

Verified on 2026-09-30: focused cache tests and `cargo make ci` passed, including
backend context conformance. The existing Shardline-style context runner returned
9/9 fixed targets at both 2,048 and 8,192 tokens across three fresh Turso runs.
Evidence: ignored `target/context-retrieval-results/context-20260929T214110.359337Z-uncommitted/`.
Recorded medians were 778,831 µs and 2,195,075 µs; the evaluation overlapped CI
and is not an isolated performance comparison. Exact tokenizer packing remains
a separate cost, and no whole-query speedup is claimed.

Verification follow-up: the context compiler's substantial unit suite lives in
`crates/syntaxmesh-query/src/context/tests.rs`, following the project module-layout
rule. An additional cache test fills the aggregate 8 MiB capacity with two
accepted versions, proves subsequent text is not retained, and proves existing
entries remain reusable without further provider reads.
