# ADR-0214: Daemon Streamable HTTP MCP

- Status: accepted
- Date: 2026-09-30

Expose optional `/mcp` on the existing daemon loopback listener using the locked
MCP SDK's Streamable HTTP service and the shared mixed-language MCP host. Enable
with `--mcp` and an explicit `--context-tokenizer`. Reuse SDK session management,
protocol dispatch, request-body limits, and cancellation; do not hand-write MCP
framing or reopen the database. This is an external MCP protocol, not an internal
NDJSON graph handoff or canonical storage representation.

Let the HTTP host compose additional host routes under its existing exact Host,
Origin, Fetch-Site, and URI boundary middleware. The SDK additionally
receives an exact allowed authority, rejects present Origins, and caps request
bodies at 64 KiB. Disable SSE keepalive and cancel SDK sessions on daemon shutdown.
The existing watch worker and retained Engine/runtime drop ordering stay intact.

Do not use stdio for this long-running multi-surface daemon: the SDK uses Tokio's
blocking stdin reader, which complicates termination while a client leaves stdin
open. The separate standalone stdio executable remains available. This provides
daemon MCP serving, not local ownership-discovery IPC or automatic CLI attachment.

## Verification

Eight daemon process tests pass. With both native and polling watch, the real
SDK client initializes a session,
performs a positive search, observes a new generation after a watched edit, and
keeps its session open across Unix SIGTERM and observes graceful daemon exit
before client cancellation. Request fixtures reject foreign Host,
Origin, cross-site Fetch-Site, and bodies exceeding 64 KiB. Missing tokenizer
fails before storage access. Retained history passes StateChronicle auditing.

Full `cargo make ci` passes (172.19 seconds), including migration registries,
extension isolation, workspace checks, strict Clippy, dependency policy,
all-feature tests, and API documentation. The subsequently added native-watch
variant also passes with all eight daemon process tests, strict daemon Clippy,
and workspace formatting. Existing allowed dependency-policy warnings remain.
