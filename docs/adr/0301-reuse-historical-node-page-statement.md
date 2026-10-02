# ADR-0301: Reuse the historical node page lookup statement

Status: implemented; performance validation pending.

Historical node scans currently execute the same temporal-tree page lookup
through the connection for every missing page. Reuse the existing prepared
statement pattern from the Turso historical incidence reader instead: prepare
once per scan and call the existing `read_many_prepared` helper for each page.

Keep generation/schema validation, key ordering, payload checks, exclusive
cursors, lookahead and predicate behavior unchanged. Keep the existing bounded
per-yield page cache reset; do not retain an entire generation or introduce
another cache framework. This is a private adapter optimization with no public
contract change or new dependency.

The ADR-0300 current-Shardline host evaluation was already compiled before this
change and remains baseline evidence. It scans 842 files with fingerprint
`157bbd788278ffcc5cdfd0b2de87df3038afeec3dca86fe0a3346099f00cb672`, differing
from the earlier 828-file corpus. Neither its timings nor this optimization may
be presented as an unchanged-input comparison with that earlier run. Prepared
statement reuse alone is not proof that cold index construction is acceptable.

All 41 Turso unit tests, strict all-target Turso Clippy, workspace formatting
and the architecture dependency check pass. All 17 cross-backend conformance
scenarios pass in 41.66 seconds, including historical node paging, updates,
retained generations and durable restart. Full contributor CI passes in
205.66 seconds with two build jobs, including all-feature workspace tests,
strict Clippy, migration/extension gates and documentation. Existing allowed
dependency-policy warnings remain; no warning-free audit is claimed.
Representative performance validation remains pending.

Explicit selected-file Sim indexed-host smoke
`context-20261002T102608.098152Z-uncommitted` passes all five required
8,192-token targets on the unchanged five-file fingerprint in 8.20 seconds.
Cold context takes 505.019 ms; retained 8,192-token requests take
34.203–890.833 ms. These are single small-fixture samples, not repository-scale
speedup evidence. Artifact `context-20261002T102514.367754Z-uncommitted`
also passes, but selected the engine-only evaluation because no external root
was supplied; it must not be described as indexed-host validation.

Whole-Shardline actual-host rerun
`context-20261002T102712.399141Z-uncommitted` is running with the existing
`TMPDIR` mechanism directed to a fresh directory under SyntaxMesh's `target/`
on disk, outside the analyzed source root. Artifact metadata records the scratch
root and source fingerprint. It prepares 844 files with input fingerprint
`caa25a0717be4030be3e46341ac77b8eb6a8c23c5d29c9b8deabb5b4511e32f1`;
the Shardline corpus changed again, so this is current-corpus validation, not
an unchanged-input speedup comparison with either earlier run. `/tmp` reports
a 25,356-MiB user quota; the disk-backed run avoids that quota without deleting
existing temporary data. It includes statement reuse and cannot serve as a
pre-change baseline.

The disk-backed run subsequently completes publication in 970.761 seconds,
clearing the stage at which the previous tmpfs run reported quota exhaustion.
The process remains live for host setup and retrieval. This supports the chosen
scratch-storage workaround but does not prove unchanged-input performance or
close the retrieval/cold-query gates.

The same disk-backed run completes successfully in 1,357.01 seconds. All five
required 8,192-token targets are retrieved with one hop/64 candidates, including
Python metadata and both documentation cases. Across both budgets it retrieves
8/10 targets; Rust webhook and benchmark documentation miss at 2,048 tokens.
Cold context takes 182.823160 seconds including index construction. Retained
8,192-token requests take 0.902398–1.062620 seconds in this single debug-mode
sample. The temporary fixture automatically releases its database on completion;
raw evidence remains in the artifact directory. Together with whole-Sim this
supports the explicit opt-in host's required-target gate on these corpora, not
general relevance, an SLA, default promotion or acceptable cold-query latency.
The changed corpus and scratch medium prevent attributing the timing difference
from the earlier diagnostic run solely to prepared-statement reuse.
