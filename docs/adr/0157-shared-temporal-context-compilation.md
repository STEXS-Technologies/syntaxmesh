# ADR-0157: Shared temporal context compilation

- Status: accepted
- Date: 2026-09-30

## Decision

Add explicit generation-pinned Query::historical_context. Share the ordinary
compiler's selection, source hash validation, provenance classification, and
exact-token packing. A focused internal read adapter selects current or temporal
node/search/adjacency/provenance ports; do not duplicate the context algorithm.
Keep Query::context on its current-generation path and its freshness behavior.

Historical compilation looks up only file identities referenced by selected
nodes, using the indexed historical_file port. It uses existing historical
adjacency pages, retained provenance point reads, and generation-root search.
No replay, snapshot fallback, inference, workflow, or new dependency is added
to the durable read path. Reference stores retain their existing fallback ports.

Source bytes remain host-injected: changed current files yield stale-source
warnings and no quotations; matching archived bytes supplied by a host can
produce verified snippets. Historical graph facts alone do not imply historical
source availability. Empty/unknown/missing seeds retain existing error semantics.
Context DTO schema v2 is unchanged and already identifies the selected generation.

This does not yet expose historical selection in MCP (ADR-0154), make substring
search constant-time, or bound total high-degree adjacency work beyond existing
context limits. Those limitations must remain explicit.

## Verification

Verify current/historical parity at the same generation, historical facts after
an edit, source-hash rejection, seeded and lexical selection, provenance classes,
serialized token budgeting, cross-backend parity, and durable restart.

Those fixtures pass across all four backends, including a file-content/hash and
producer/evidence-class update. The actual repository-reader scenario verifies
stale and matching source bytes while ordinary current-context behavior remains
unchanged. Full `cargo make ci` passes. MCP generation selection remains open.
