# ADR-0171: Local command semantic evaluation

- Status: accepted
- Date: 2026-09-30

## Decision

Extend the existing opt-in semantic-provider evaluator rather than creating a
second evaluation service or workflow. Accept exactly one explicit transport:
`SYNTAXMESH_SEMANTIC_EVAL_ENDPOINT` or `SYNTAXMESH_SEMANTIC_EVAL_COMMAND` (JSON argv
or `@path`). Command mode defaults to GPT-6 Luna, accepts the existing model and
revision overrides, and forwards model selection through the normal CLI adapter.
Resolve relative command-file paths before entering the retained run directory.
Reject transport conflicts and command/HTTP network-option conflicts before work.

Share initial, cached-repeat, sparse-edit, and edited-repeat execution, source
hash/evidence review, and historical/cache checks. Report v5 identifies transport
without recording command arguments, endpoints, credentials, or auth settings.
Command token usage remains explicitly unavailable. Keep failure logs for review;
do not grade malformed output, label presence, or exact quotes as model precision.

Reuse the existing Rust executable fixture to exercise command evaluation in CI.
Live evaluation remains opt-in and potentially billable; CI does not run models.
No core, DTO, storage, engine, or provider-specific API contract changes.

The cargo-make task forwards its existing workspace working-directory variable
through `SYNTAXMESH_SEMANTIC_EVAL_WORKING_DIRECTORY`, because Cargo launches test
processes from the crate directory. Direct test invocations may set that origin
explicitly or use absolute command-file paths.

## Verification

HTTP fixtures retain their original grounded/empty and cross-document scenarios.
The shared command fixture covers both evaluation modes, relative argv files,
model overrides, exact supporting source, selective edits, unchanged repeats,
and retained historical claims. Unit checks reject ambiguous/missing transports,
command network conflicts, and empty model/revision settings.

The documented cargo-make invocation completed a live GPT-6 Luna/medium,
document-local run under `target/semantic-provider-results/run-8lekPn`:
six initial claims, seven after the edit, 12.714583-second initial execution,
10.808019-second edit, and 0.047976/0.046768-second cached repeats. All three
cache/history checks passed. These are end-to-end timings for two short controlled
documents, not a Graphify comparison or representative workload benchmark.

Manual review found subject naming drift (`the change engine` versus
`change engine`) and relation wording drift (`exclude` versus
`exclude_dependencies`) after re-extraction. Those remain distinct inferred
facts rather than silently normalized aliases. Exact quote matching does not
establish entailment; one rationale claim quotes only the consequence clause.
Representative coverage/precision, consistent concept naming, and richer evidence
remain open quality gates. No automatic quality score is inferred from this run.

Combined `cargo make ci` passes on the final implementation (110.13 seconds),
including strict Clippy, dependency/runtime boundaries, migration registries,
extension compatibility, workspace tests, security policy, and documentation.
