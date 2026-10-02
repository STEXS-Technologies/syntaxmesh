# ADR-0154: Explicit historical MCP context

- Status: accepted; implemented on top of ADRs 0155–0157
- Date: 2026-09-30

## Decision

Expose a generation-pinned temporal context path through an optional
64-hex-character `generation` argument on the existing MCP context tool. Omit
it to preserve the startup-generation behavior. Require the requested manifest
to belong to the server's repository and worktree. Unknown IDs, malformed IDs,
and other scopes fail; never silently substitute the current graph.

Retain startup-generation freshness checks before and after each operation.
Historical selection does not authorize a stale MCP process to keep serving
after a publication: restart still selects the current scope and snapshot.
The server must remain read-only and must not reconstruct graph state by replaying
indexing or invoking inference. The attempted host-only implementation failed:
`Query::context` uses current-only file, search, adjacency, and provenance reads,
so binding it to an earlier generation fails with StaleBase. Existing indexed
historical node/adjacency APIs alone did not supply the missing context inventory
and source/provenance reads. ADRs 0155–0157 now supply explicit
`Query::historical_context` using temporal reads. Route explicit MCP selection
through that path, validate its manifest scope, and preserve the omitted-argument
route. Full snapshot materialization is not accepted as a substitute.

Source access remains the repository-scoped current-filesystem reader. Historical
source bytes are not archived or fetched from Git by this increment. Changed
bytes therefore produce stale-source warnings and no snippets, while historical
graph facts/provenance remain selectable. An available matching source file can
still provide verified snippets. Explicitly distinguish this from complete
historical source retrieval or arbitrary time-range synthesis.

Apply Shardline's focused sibling layout to the existing large MCP unit suite:
move unchanged tests to `server/tests.rs`, and put historical-context scenarios
in its dedicated child module. Keep implementation out of lib.rs/mod.rs.

## Verification

The original regression proved the current-only limitation after a durable edit;
the existing current MCP tool remains usable. Implementation must cover
explicit earlier generation versus default current generation, matching
and changed source bytes, unknown/malformed selection, and unchanged stale-host
failure. Verify exact budgeting and provenance classification via existing tests.

The limitation regression and unchanged current-context behavior pass. All
original MCP test bodies are preserved after formatting normalization, and the
host implementation/public contract is unchanged by the suite move. Full
`cargo make ci` passed for that initial suite move. The subsequent ADR-0157
fixture additionally exercises explicit temporal context with changed and
matching repository source bytes. MCP selection tests now cover omitted versus
explicit current generation, retained selection after restart, stale versus
matching sources, malformed and unknown IDs, typed provenance and budgeting,
and refusal to serve from a stale host. Scope-guard unit tests exercise both
repository and worktree mismatch; schema checks keep generation optional.

The stdio integration fixture now publishes an initial graph and an edited graph
before starting the MCP process. Actual `tools/call` requests verify omitted and
explicit current selection produce identical packs, while explicit retained
selection returns the earlier generation and caller facts, excludes the later
symbol and source snippets, and reports `stale_source` for changed filesystem
bytes. Token budgeting remains enforced. This closes the historical-selection
wire-path gap rather than relying only on direct server calls.
Full `cargo make ci` passes with this fixture, including strict workspace Clippy,
architecture and migration guards, backend conformance, and the stdio test.
