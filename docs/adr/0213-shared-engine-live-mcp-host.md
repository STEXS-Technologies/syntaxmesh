# ADR-0213: Shared-Engine live MCP host

Reuse HTTP's shared-Engine composition for MCP: support a Send-capable extractor
while retaining Rust as the standalone default and retaining standalone startup
pinning. A trusted owner may supply the existing Turso Engine and exact scope;
default reads select its current generation under the same mutex as publication.
Response labels come from the admitted query, not the startup field. Explicit
historical context selection retains existing scope validation.

Caller owns lease, recovery, publication, source-root correctness, and runtime
lifetime. No second database connection or new graph semantics are introduced.
This enables daemon MCP composition but does not yet expose a daemon transport,
local IPC, or CLI attachment. Reads still serialize with publication.

Use the SDK's separate `tool_router` and `tool_handler` macros for the generic
host: the combined `server_handler` expansion duplicates type generics in the
currently locked SDK. No SDK fork or hand-written protocol dispatch is needed.

Verified: twelve MCP tests pass (two manual retrieval evaluations remain ignored).
The mixed-source fixture verifies refreshed search/status/node/neighbor/path
labels, current and historical context, retained historical search, foreign-scope
rejection, and StateChronicle history on the same Engine. Existing standalone
startup-pin rejection remains green. Strict MCP Clippy, formatting, and
architecture checks pass. Full workspace CI passes with ADR-0212's daemon
failure fixtures and this shared MCP change (155.10 seconds), including locked
extension import, migration registries, all-feature compilation and Clippy,
workspace tests, backend conformance, and documentation. Manual retrieval and
live-model quality evaluations remain opt-in; this run does not establish them.
