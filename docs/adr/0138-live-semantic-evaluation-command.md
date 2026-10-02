# ADR-0138: Opt-in live semantic evaluation

- Status: accepted
- Date: 2026-09-30

Reuse the existing Rust CLI process-fixture and retained benchmark-evidence
approach for `cargo make evaluate-semantic-provider`. An ignored integration test
requires an explicitly configured provider endpoint, creates two controlled
architecture documents in a fresh retained run directory, indexes them through
the normal one-command semantic workflow, and repeats unchanged inputs to check
durable cache reuse and generation stability.

Retain fixture documents, CLI stdout/stderr, the durable snapshot, source hashes,
and a JSON report of accepted claim labels/provenance. Never record API keys or
raw endpoint configuration. Remote HTTPS still requires an explicit evaluator
network opt-in passed through the normal CLI policy. Do not download models,
start model services, or add an SDK/evaluation service.

Report label presence for Penelope, StateChronicle, and Change Engine only as
a controlled-fixture recall signal. It is not semantic precision, relationship
correctness, or real-project quality; reviewers must inspect accepted claims and
their stored source evidence. Zero claims remains visible, not silently graded
as good quality. Provider failure and cache instability fail the evaluation.
CI compiles but never executes this ignored, potentially billable test.
An automatic loopback mock verifies harness/cache plumbing and explicitly
reports empty output without claiming useful extraction. The runtime audit
permits only the exact `std::env::current_exe()?` Rust self-launch inside test
paths; production and arbitrary executable names remain rejected.

No core/public DTO/cache-format changes. Real-model quality remains unverified
until a configured provider has actually completed the evaluation.
