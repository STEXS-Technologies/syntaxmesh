# ADR 0381: MCP reads for retained contract history

Status: accepted

Date: 2026-10-22

## Context

ADR-0380 introduced indexed, generation-scoped contract-result history behind
the inference SQLite materializer and Engine. MCP can evaluate contracts on a
request, but callers cannot inspect results already retained by the operator.
Request-time evaluation is not a substitute: it neither proves that a result
was retained nor avoids reevaluating the selected graph.

## Decision

Add a read-only MCP tool that pages retained results for one operator-installed
inference binding and exact contract identity/version. The binding supplies the
database and host scope; callers cannot choose filesystem paths or repository
scope. Pages traverse the chronological result set using the materializer's
indexed keyset cursor, enforce host ceilings, and return retained contract,
evaluation, and evidence as stored. Missing rows remain missing; the tool does
not evaluate, replay, migrate, or materialize.

The tool is feature-gated with retained inference reads and uses the existing
MCP schema version. Core, Query, and public storage-independent DTO crates do
not acquire database or transport dependencies.

## Consequences

- Harnesses can inspect durable contract history without triggering inference.
- Results are visible only after explicit materialization and migration.
- Contract authoring, automated successor evaluation, and drift summaries
  remain separate roadmap work.

## Verification

Verify binding-scope admission and cursor substitution rejection, bounded
pagination, empty/unmaterialized history, tool discovery, and stdio
serialization. Confirm reads do not change the database.

## References

- ADRs 0378–0380.
- SyntaxMesh source of truth v5, sections 301, 311, 316, 317, and 320.
