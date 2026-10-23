# ADR-0398: Explicit indexed-context option for standalone MCP

- Status: accepted
- Date: 2026-10-23

## Context

The standalone MCP host default uses bounded substring lookup. The indexed
context path produces better fixed-target retrieval on larger repositories,
but builds a generation-scoped in-memory planner on the first context request.
In one full Shardline run, indexed context retrieved all seven fixed targets at
both 2,048 and 8,192 tokens. Its cold build scanned 266,225 nodes and 2.74
million postings, took 49.9 seconds, and retained about 97 MB of charged planner
payload; the first request took 50.8 seconds, while subsequent requests in the
same host took about 0.7–2.0 seconds. The default route missed `load_metadata`
at both budgets in the same repository scope.

The CLI currently exposes indexed mode indirectly through
`--lexical-first-context`, which changes both the index choice and discovery
ordering. Callers need a direct way to choose indexed context with the tested
balanced policy. The measured cold cost is not suitable evidence for silently
changing the default.

## Decision

Add a standalone MCP `--indexed-context` launch option. It enables the existing
generation-scoped indexed planner with balanced discovery and neutral source
roles. Keep indexed context opt-in. Preserve `--lexical-first-context` as an
explicit alternative indexed policy; reject passing both policy flags together.
Allow `--source-content-context` to compose with either indexed policy as it
does today. Explicit seed requests continue to use their existing path.

## Alternatives

- Make indexed context the default: rejected because a full-repository cold
  build took about 50 seconds and only one full-corpus sample is available.
- Keep indexed mode available only through `--lexical-first-context`: rejected
  because it also changes discovery order and does not expose the measured
  balanced policy directly.
- Add a new query DTO switch: rejected because this is host policy, not a
  runtime-neutral query or persisted graph concern.

## Consequences

The standalone MCP command gains one opt-in flag; default behavior, public DTOs,
graph persistence, and the library host defaults remain unchanged. The planner
is still built lazily and retained per configured host. Repository-scale
quality/cost evidence remains one fresh run and does not establish universal
relevance or performance.

## Verification

The full Shardline fixed-target evaluation passes 7/7 targets at both token
budgets under indexed balanced discovery. The CLI parser and stdio protocol
tests verify the option and its behavior. This is not a default promotion or a
cross-repository release gate.
