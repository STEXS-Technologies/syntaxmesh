# ADR-0183: HTTP generation and recovery readiness

Status: Accepted

## Decision

Reuse Shardline operational.rs separation of liveness and dependency readiness,
including redacted failures. GET /readyz returns schema-1 JSON with ready/not_ready
status, the startup generation as hex, and read-only recovery diagnostics:
prepared, completed, and rejected operation counts. Prepared operations produce
503 with those diagnostics; stale generation, exhausted/closed admission, or
diagnostic errors produce the existing redacted 503 error body. Healthz remains
process liveness, not dependency health.

Use existing Engine workflow_diagnostics and current-generation checks inside
the same admitted blocking worker as other queries, with pre/post pin validation.
Do not invoke recovery, modify workflows, run full graph integrity checks, or
scan source paths. Workflow diagnostics enumerate durable operations: this is
not a constant-time probe or a full daemon reconciliation guarantee. Rejected
terminal operations are reported but do not alone block read readiness.

No change to core/DTO/protocol contracts, storage, or Penelope orchestration.

## Verification

Real TCP tests for both tokenizer configurations compare readiness generation
and counts against Engine diagnostics, then exercise exhaustion/recovery and
startup staleness; retained host clones also reject readiness after shutdown.
A focused response-policy test covers prepared and terminal rejection counts.
It is not an interrupted durable-work TCP recovery fixture. Full cargo make ci
passed on 2026-09-30 in 124.73 seconds.
