# ADR-0317: Reuse the release evaluator on the Rust-heavy sibling corpus

Status: moving-worktree repetition rejected; separate committed-HEAD repetition verified.

The expanded six-case matrix completes in
`context-20261002T143205.829581Z-uncommitted` on the same 808-file fingerprint
and unchanged index counts. It adds Rust pending-migration lookup without
removing previous cases. The new target is retrieved at both budgets (1,952
and 8,082 packed tokens); all six required larger-budget checks pass, and four
of six smaller-budget checks pass. Existing webhook and heading omissions remain.
Publication takes 43.812 seconds; the now-first migration query is cold at
2.763703 seconds, with 2.724783 seconds index construction. Warm larger-budget
queries span 42,188–65,173 microseconds. Parent run completes in 84.72 seconds.
Different query order prevents isolated comparison with prior cold webhook
measurements. Regular MCP/harness tests and strict Clippy pass; full contributor
CI passes in 139.57 seconds for the test-module move and matrix/count changes,
including workspace all-feature tests, strict Clippy, architecture/extension
checks and documentation. Production ranking remains unchanged; this closes
verification of the evaluator changes, not the remaining retrieval-quality gate.

The committed-HEAD follow-up completes in bundle
`context-20261002T142505.146421Z-uncommitted` (190.81 seconds parent time).
All three fresh databases scan 808 files with identical fingerprint
`4419eb47c3925327b409c61efc391c8886aca746badb7fe664e4daae8c143795`.
Each index contains 202,664 nodes, 2,067,222 postings and 72,907,891 payload
bytes. Publication is 49.374, 43.461 and 44.145 seconds (median 44.145).
Cold context is 2.685947, 2.673021 and 2.667162 seconds (median 2.673021).
Index construction is 2.629910, 2.626420 and 2.620749 seconds.
Warm 8,192-token queries span 41,180–65,401 microseconds; all fifteen required
target checks pass. Smaller budgets retain nine of fifteen checks. Metadata
confirms the clean detached fixture revision, release profile and source identity.

This verifies limited same-host repetition on a Rust-heavy committed corpus,
including documentation-rationale content. It does not cover the sibling's
uncommitted edits, complete language semantics, AI inference quality, concurrent
serving or percentile latency. Keep the failed moving-corpus bundle separate;
no isolated algorithmic speedup or default promotion is established.

For a separate stable-corpus follow-up, create a uniquely owned local Git clone
with `--no-hardlinks`, then detach its checkout at committed HEAD
`d91b58b65b6f`. The clone is under ignored
`target/shardline-context-corpus.9z2KgbZE/shardline`; its worktree is clean.
Run the same three-sample whole-repository release evaluator against that clone.
This borrows Git's ordinary immutable revision workflow, not a new copying
service or relaxed fingerprint gate. It intentionally excludes uncommitted
sibling edits and is a distinct corpus baseline, not validation of those edits.
Original sibling state is untouched. Clone and raw logs are retained for audit;
no cleanup or deletion of another project's files is performed.

Bundle `context-20261002T141943.232032Z-uncommitted` fails closed after its
second sample because 908 selected files change from fingerprint
`9fe20cc56bd804dd76abae8facffc603341b53ac27e5cbfbb3e0d529c8c9489d`
to `a0cac5653dc265a1495545174a57094adfd0519228a21d4f6e11cba355307eac`.
The evaluator does not start a third sample or publish an accepted aggregate.
Do not derive repeatability or speedup from this bundle.

The first completed raw sample retains all five required 8,192-token targets,
including documentation rationale content, and three of five at 2,048 tokens.
It indexes 220,883 nodes, 2,509,195 postings and 87,685,346 payload bytes.
Publication is 52.901 seconds; context-index construction 3.061480 seconds;
cold context 3.119448 seconds. Warm required-budget queries range from
44,150 to 67,649 microseconds. These are single-sample observations only.
The second index differs (220,885 nodes), confirming the input mismatch affects
the graph. Future repeated evidence needs stable source inputs or an explicitly
provenance-captured immutable corpus copy, not suppressed fingerprint validation.

Reuse the existing Shardline-inspired provenance/evidence harness for whole
Shardline source ingestion and the established five-query target matrix. Run
three fresh databases in release mode with indexed-host routing explicitly
opted in, exact o200k_base token accounting and the same scratch parent as
ADR-0315. Do not execute analyzed files or modify the sibling worktree.

The sibling is dirty at invocation. Capture its revision, dirty flag and actual
scanner-selected content fingerprint; acceptance requires the same fingerprint
across samples. Do not compare against older Shardline timings as isolated
speedups if inputs differ. Cases cover Rust webhook logic, Python metadata,
Bash querying, a documentation heading and documentation rationale content.
The latter checks document content rather than only links or headings.

Record target retention, exact token bounds, index counts, cold/warm context
latency and publication cost. Missing required targets fail the existing gate;
do not weaken it or silently narrow to selected files. Fixed-target success is
limited evidence, not comprehensive language semantics, AI extraction quality,
cross-machine latency or default routing promotion.
