# ADR 0386: Bounded contract outcome episode timeline

- Status: accepted
- Date: 2026-10-22

## Context

ADR-0385 materializes indexed episodes for repeated contract outcomes, but only
an as-of lookup is exposed. Consumers that need the sequence of observed
outcome changes still have to read every retained contract evaluation. The
episode projection already holds the compact transition boundaries.

## Decision

Expose a bounded, chronological, keyset-paged read of the existing episode
projection, scoped to one repository, worktree, contract ID, and version. The
cursor carries that complete scope and the prior episode's first sequence.
Every returned boundary is verified against its retained evaluation before it
is returned. The page is a read of the latest retained history, not a
generation-pinned conformance query; callers needing a pinned answer continue
to use the as-of operation.

This is a downstream inference-store/query feature. It adds no canonical graph
schema, migration, runtime dependency, automatic evaluation, lifecycle status,
or policy semantics. Episodes report observed evaluations only; gaps are not
evidence of continuous state.

## Alternatives

- Page all retained evaluations and fold outcomes in clients: rejected because
  the projection already stores the exact compact answer.
- Treat an episode page as an as-of answer: rejected because later observations
  could leak into a historical interpretation.
- Add episode data to the canonical graph: rejected because the projection is
  downstream inference history and must not couple core storage to policy.

## Consequences

Clients can read outcome transitions in bounded pages without canonical replay
or per-evaluation history scans. Existing exact result history and as-of
semantics remain unchanged. The cursor is opaque only by convention and must
be treated as scoped data; malformed or cross-scope positions fail closed.
