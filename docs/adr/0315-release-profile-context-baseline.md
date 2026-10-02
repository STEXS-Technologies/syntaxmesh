# ADR-0315: Measure the restored reader in the existing release harness

Status: first baseline and three-sample same-host repetition complete; broader quality pending.

Three fresh-database repetitions complete in bundle
`target/context-retrieval-results/context-20261002T140055.034369Z-uncommitted`.
All match the 2,572-file fingerprint and identical node/posting/payload counts.
Cold context is 1,110,087, 1,036,111 and 1,010,561 microseconds (median
1,036,111). Index builds are 1,069,585, 999,735 and 973,658 microseconds.
Publication is 17,268, 16,312 and 14,782 milliseconds (median 16,312).
Across the fifteen warm 8,192-token queries, latency ranges from 35,139 to
59,449 microseconds. Every repetition retains all five required targets at
8,192 tokens and three of five at 2,048 tokens. Parent run completes in
70.86 seconds with the release build already available.

This establishes limited same-host consistency for this fixed target matrix,
not tail-latency, concurrency, cross-machine behavior or comprehensive semantic
quality. No extra cache or default routing promotion is justified. The next
quality work must address representative query coverage and known small-budget
omissions rather than treating repeated timings as product completeness.

Bundle `target/context-retrieval-results/context-20261002T135502.154677Z-uncommitted`
completes successfully. Prepared inputs match the prior 2,572-file fingerprint
`d085ad193619f9cb5957280fc2d47f93562a3ea6156c0baa69ecaf1710c91d24`.
Publication takes 17.663 seconds; evaluation takes 24.85 seconds excluding the
4m34s release prebuild. Parent execution takes 299.93 seconds including build.
The index retains 87,486 nodes, 567,068 postings and 21,071,384 payload bytes.
Index construction is 987,029 microseconds; cold context is 1,033,233 microseconds.
Warm 8,192-token queries take 53,286, 51,088, 38,650, 40,245 and 36,384
microseconds. All five required targets are retrieved; 2,048-token queries
retain three of five, preserving the known Python/documentation omissions.

The demand-driven scanner loads 90,556 pages and decodes 87,486 nodes in
986,897 microseconds. Schema processing is 181,499 microseconds; SQL page
loading 448,995, including transfer 385,498 and envelope decoding 52,508.
Validation is 76,566 and node decode 45,464 microseconds. This confirms removal
of the rejected batch amplification in the optimized path. Do not compare this
with debug timing as an isolated code optimization. One fresh-database sample
does not establish percentile latency, concurrency behavior, broader relevance
or readiness to enable indexed-host routing by default.

ADR-0314 rejects speculative batching and its removal passes full CI. Before
adding another cache, reuse the existing provenance-captured context evaluator
with `SYNTAXMESH_CONTEXT_BENCH_PROFILE=release`. Keep whole-Sim repository scope,
one fresh temporary database, indexed-host profiling, tokenizer, scratch parent
and target matrix unchanged. Verify the prepared input fingerprint before
interpreting results. Release and debug times are different build policies;
their comparison cannot establish an algorithmic speedup.

Shardline's `shardline-cache/src/memory/inner.rs` provides byte-accounted FIFO
eviction, and `memory/cache.rs` evicts until entry and byte limits are satisfied.
Do not copy this yet: the prior demand-driven scan loaded 90,556 pages for
87,486 nodes, so eliminating all excess reads alone would cover only a small
fraction of page transfers. SyntaxMesh's tree cache also exposes no selective
eviction contract. Adding one would require its own decision and evidence.

Capture build profile, exact inputs, index counts, required target retention,
cold and warm latency, and stage timings. One sample is diagnostic, not
repeatable performance acceptance or general relevance. No default promotion,
cache policy, public contract, storage format or runtime dependency changes.
