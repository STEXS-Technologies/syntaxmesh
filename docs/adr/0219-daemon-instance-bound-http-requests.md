# ADR-0219: Daemon instance-bound HTTP requests

- Status: accepted
- Date: 2026-09-30

## Decision

Configure the shared HTTP host with the daemon's validated 64-character lowercase
hex owner-instance ID. In the existing outer boundary middleware, an optional
`x-syntaxmesh-owner-instance` request header must occur exactly once and match the
configured instance. Reject mismatches, duplicates, or guards sent to an
unconfigured standalone host with HTTP 412 before route dispatch. Return the
configured instance in response headers so attached clients can verify it.

Ordinary requests without the header remain compatible. This is accidental stale
instance protection, not authentication or a secret. Existing exact Host, Origin,
Fetch-Site, URI, and MCP body/session boundaries remain intact. The middleware
also covers additional MCP routes, without changing MCP messages or public graph
DTOs. Attached clients must supply and verify the header on every request.

Create one fresh OwnerEndpoint under the daemon's retained writer lease. Publish
its atomic discovery record only after native watch registration and successful
initial reconciliation, before readiness output. Publication failure stops startup
and joins the watcher through the existing cleanup path. Keep the lease outside
the async future so it outlives writer/runtime/Engine teardown. Stale records may
remain after termination; the active-owner reader ignores them. CLI attachment
is still separate work.

## Verification

Ten daemon process tests pass. Discovery fixtures verify published listener
identity, a positive guarded search with matching response header, duplicate
guard rejection, restart instance changes, stale-instance rejection, and ignoring
records after shutdown. Publication failure reports no readiness and releases
ownership. Native/polling MCP fixtures reject invalid instance guards before SDK
dispatch. Eight HTTP tests pass, including guarded requests failing closed on
unconfigured standalone hosts. Strict HTTP/daemon Clippy, architecture checks,
and formatting pass. This is not CLI attachment or hostile-listener authentication.
