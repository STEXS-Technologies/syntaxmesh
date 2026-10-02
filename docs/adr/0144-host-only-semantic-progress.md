# ADR-0144: Report semantic inference progress without changing graph output

- Status: accepted
- Date: 2026-09-30

## Context

Graphify's extraction CLI reports completion of inference chunks. SyntaxMesh's
one-command semantic index currently reports only final totals, so a long local
inference run can appear stalled. Existing Rust CLI counters already separate
provider work, usage, cache reuse, and graph publication.

## Decision

Keep progress in the CLI host. Before uncached inference, emit bounded aggregate
pending-request, prompt-group, and parallel-limit counts to stderr. Emit one
validation-result line when each original prompt group completes, directly from
its worker rather than waiting for every worker in a wave. Successful counts
mean source-evidence validation, not durable caching or graph publication.

Do not include source paths, content, credentials, endpoint URLs, model names,
provider error bodies, or estimated durations. No progress lines are emitted for
empty pending work, including full cache reuse. Keep stdout and graph/export
DTOs unchanged; add no flags, UI dependencies, progress records, or core hooks.
Parallel completion order is intentionally unspecified. This is human-facing
diagnostic text, not a versioned telemetry protocol or billing history.

## Verification

CLI recovery fixtures must observe aggregate progress while uncached work is
executed and no progress on a provider-free warm cache repeat. File and verified
Turso must retain the same existing graph/publication assertions.

The recovery fixtures cover both all-success counts and a mixed successful/
failed group, followed by a warm repeat with no inference-progress lines.
