# ADR-0149: Evaluate the cross-document semantic layer explicitly

- Status: accepted
- Date: 2026-09-30

## Decision

Extend the existing opt-in semantic-provider evaluator, not the provider or core
contracts. `SYNTAXMESH_SEMANTIC_EVAL_CROSS_DOCUMENT=true` selects the existing
two-layer indexing command; absent/false retains document-only evaluation.
Reject other values before inference. Like Shardline's benchmark evidence
bundles, retain separate cold/warm logs and timing with the selected workload.

Report schema v3 records the selected mode, expected cache units, cold/warm
end-to-end indexing durations, and the number of accepted claims whose verified
quotations reference multiple distinct source paths. Reuse existing exact source
hash/span review. Cache stability still requires zero warm inference requests
and unchanged generation/history. Cross-document mode expects three cache units
for this fixed two-document corpus rather than two.

These counts are structural evidence, not precision, recall, entailment, or a
Graphify speed comparison. Empty inference is valid and reports zero joint
claims. No model is started or downloaded; live inference remains opt-in and
requires an explicitly supplied endpoint. No database/public DTO change occurs.

## Verification

Controlled providers exercise document-only empty/grounded output and two-layer
output with a two-source claim. Verify retained quotations, timings, mode/cache
counts, and warm generation stability. Real-model quality remains an open gate.

Five controlled evaluator tests pass: document-only empty/grounded output,
two-layer grounded/empty output, and invalid mode rejected before workload setup.
Full `cargo make ci` passes, including strict Clippy and architecture checks.
The live evaluator remains ignored in CI and was not run against a real model.
