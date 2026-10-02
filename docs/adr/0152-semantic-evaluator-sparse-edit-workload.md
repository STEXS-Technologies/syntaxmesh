# ADR-0152: Semantic evaluator sparse-edit workload

- Status: accepted
- Date: 2026-09-30

## Decision

Reuse Shardline's initial/sparse-update benchmark separation from
`scripts/benchmark-matrix.sh` in the existing Rust semantic evaluator. After
initial extraction and unchanged cache verification, append one paragraph to
the ownership document, enrich again, and repeat the edited input unchanged.
Keep original source bytes separately for the initial claim review. Retain all
four invocations' logs, end-to-end timings, edited claim review, and original
and edited content hashes in report schema v4.

Require one unchanged document cache unit on edit in both modes. Document-only
mode submits one request; cross-document mode submits the edited document and
its affected compound request. HTTP attempts can exceed submitted requests due
to bounded recovery/retry; do not equate those counts. Require positive edit
inference, a new generation, retained initial history, and zero inference/no
new generation on the edited repeat. This remains an opt-in provider workload
and adds paid inference when a remote provider is explicitly selected.

No new benchmark framework, language runtime, provider, core contract, or
mutation workflow is added. The evaluator modifies only its generated fixture.
These are cache/publication checks, not precision, entailment, or speed scores.

## Verification

Exercise empty and grounded document-only/cross-document providers, checking
actual request inputs and retained reports. Real-model evidence remains open.

All four mock workloads pass, including edited-document-only request contents,
affected compound inference, unchanged extraction reuse, historical quote review,
retained source versions/logs/timings, and provider-free edited repeats. Full
`cargo make ci` passes with strict Clippy and architecture checks. No live model
was invoked; these results validate the evaluator and cache lifecycle only.
