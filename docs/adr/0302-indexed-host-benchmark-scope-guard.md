# ADR-0302: Reject indexed-host benchmark flags on the engine-only fixture

Status: implemented and targeted validation passed.

The existing benchmark chooses its engine-only test when no external evaluation
root is supplied. That test does not enable the sibling evaluator's indexed-host
policy. Recording `SYNTAXMESH_CONTEXT_INDEXED_HOST_PROBE` as enabled in this
combination is misleading even when the engine-only test passes.

Reuse the existing early benchmark scope-validation boundary: reject that flag
unless `SYNTAXMESH_CONTEXT_EVAL_ROOT` is supplied, before creating artifacts or
launching builds/tests. Keep the unflagged engine-only fixture and external-root
selected/repository evaluations unchanged. Test all four root/flag combinations.
This changes contributor benchmark validation, not production retrieval APIs.

The already-running disk-backed ADR-0301 Shardline test has an explicit root;
its captured implementation predates this guard and its host policy is valid.

All 16 xtask tests, strict all-target xtask Clippy and workspace formatting pass.
The real no-root/indexed-host command rejects with the explicit scope error
before creating its requested output directory. ADR-0301 full contributor CI
predates this guard; no new full-CI result is claimed here.

Full contributor CI for this guard subsequently passes in 128.16 seconds with
two build jobs, including all-feature workspace tests, all 17 cross-backend
scenarios, strict Clippy, migration/extension checks and documentation. Existing
allowed dependency-policy warnings remain. The disk-backed repository-scale
evaluation remains live and is not represented by this CI result.
