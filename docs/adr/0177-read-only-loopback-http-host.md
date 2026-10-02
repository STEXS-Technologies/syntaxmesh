# ADR-0177: Read-only loopback HTTP host

- Status: accepted
- Date: 2026-09-30

## Decision

Add a separate `syntaxmesh-http` Rust host using crates.io Axum. Reuse Shardline's
existing-listener/injected-shutdown lifecycle and SyntaxMesh MCP's startup
generation pin, Engine queries, blocking dispatch, and runtime-drop ordering.
No transport dependency enters Engine, core, DTO, or probe crates.

The binary accepts an already migrated/indexed Turso database and a literal
loopback socket address. It never migrates, indexes, or launches source runtimes.
Routes are GET `/healthz`, `/api/v1/generation`, `/api/v1/search?text=...&limit=...`,
and `/api/v1/nodes/{id}?generation=...`. Search defaults to 20 results, bounded
to 1–100 and 4,096 text bytes. Node/generation IDs are 64 hex characters. Explicit
retained generations must belong to the startup repository/worktree. Responses
contain `schema_version: 1`, hex `generation`, and `data`; graph nodes use the
existing typed fact serialization. Missing nodes/generations return 404, invalid
input 400, changed startup generation 409, admission exhaustion 503, and internal
errors 500 without database details. Health is liveness, not graph freshness.

Check the startup generation before and after each operation, discarding results
if publication advanced; restart selects the new generation. Use at most sixteen
admitted blocking query jobs, with permits retained by the blocking job even if
the HTTP future disappears. This bounds dispatched work, not query latency or
database memory. Do not claim a cancellation timeout interrupts synchronous SQL.

Serve only on loopback; require Host to exactly equal the bound literal authority,
reject Origin and non-`none` Sec-Fetch-Site, and emit no CORS permission. These are
browser-origin/DNS-rebinding restrictions, not local peer authentication. Local
processes that can reach the listener remain trusted; multi-user/remote deployment
requires a separate authenticated policy. No NDJSON storage or service handoff.

Keep an Engine owner outside Tokio until runtime shutdown: Turso owns another
runtime. Graceful shutdown drains admitted requests; this initial host does not
claim bounded drain, producer supervision, watch refresh, daemon write ownership,
or complete HTTP/daemon acceptance.

## Verification

Actual TCP fixtures pass for logical Engine search equivalence, retained node
lookup after removal, missing/invalid inputs, browser/Host rejection, stale-
generation fail-closed behavior, and injected graceful shutdown. Exhausting all
admission permits produces 503 for queries while health remains live; releasing
them restores query service. Cancelling a query future while its blocking job is
held preserves its permit until the worker completes, then restores the full
pool. A non-loopback listener is rejected, and opening an absent database does
not create it. Focused tests, strict host Clippy, and architecture checks pass.
Full `cargo make ci` passes on this implementation (179.33 seconds), including
all-feature workspace tests, strict Clippy, migration/extension conformance,
architecture checks, dependency audits, and documentation builds. Existing
bincode/paste unmaintained-dependency exceptions remain unchanged. This is local
Linux evidence, not hosted-platform or complete HTTP/daemon acceptance.
