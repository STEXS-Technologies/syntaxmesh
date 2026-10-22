# ADR-0353: Durable inference setup intent

Status: opt-in coordinator and engine integration implemented; host delivery pending.

## Decision

Add `PenelopeInferenceSetup` as a parent journaled workflow around existing
complete-baseline and stream-initialization coordinators. Persist a bounded owned
setup request before either child is prepared. Bind full baseline request and
initialization page admission to its digest; key semantic identity by baseline
operation and destination, not execution limits. Keep all child journal bytes
and prefixes unchanged. Migration remains explicit.

Prepared parent recovery idempotently executes baseline rebuild followed by
initialization. This must work when no child was prepared, when only baseline
publication completed, and when both effects committed but parent completion did
not. Completed parent retries require retained stream initialization and matching
baseline child completion before validating child effects; missing/corrupt effects
must fail rather than be recreated. Advanced stream heads must not rewind.

Engine initialization uses this coordinator. Recovery executes pending baselines,
then parent setups, then stream initialization/successors. Add a separate `setups`
count to `InferenceRecovery`; counts describe recovered journal requests, not a
cross-database atomic transaction or canonical graph publications. Whole-command
intent is recoverable even across the gap between preparing child workflows.
Host commands and full v0–v5 completion remain separate requirements.

## Required evidence

File/Turso reopen recovery from parent-only intent, baseline-only completion and
effects-before-parent completion; immutable bounds, corrupted parent journals and
missing completed effects reject. Engine-level recovery must initialize without
requiring callers to reconstruct the original request. Existing child journal
compatibility and historical-root tests remain green.

## Implemented evidence

Thirty-three feature-enabled Penelope integration tests pass. File and Turso
fixtures reopen parent-only intent, baseline-only completion and committed child
effects before parent completion. They reject changed admission bounds, corrupted
parent records and missing completed stream effects, while preserving advanced
heads and existing child journal compatibility.

Nineteen feature-enabled engine tests pass, including recovery after a missing
downstream database interrupts setup: explicit migration followed by reopening
and recovery completes initialization without resubmitting the original request.
Strict all-target Clippy for both crates, the architecture gate and
`git diff --check` pass. These are targeted checks; the latest completed full
contributor CI snapshot predates ADR-0352 and ADR-0353.

Subsequent full contributor CI passed on 2026-10-09 in 293.87 seconds, including
all-feature workspace tests, strict Clippy and API documentation. Existing
policy-allowed warnings and opt-in ignored tests remain; host delivery is pending.
