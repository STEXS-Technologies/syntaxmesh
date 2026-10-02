# ADR-0298: Add explicit indexed context mode to the MCP host

Status: accepted for additive opt-in implementation; default routing unchanged.

Expose a consuming `SyntaxMeshMcp::with_indexed_context` host builder method.
Existing constructors remain default-off. Clones share ADR-0297's single-entry
planner cache; the setting is host-owned, not a change to ContextArgs or core
protocols. This is an explicit production-path experiment, not default promotion.

For enabled requests with no explicit seed IDs, acquire the generation-pinned
planner index inside the existing Engine-serialized query operation. Build limits
are one million nodes, eight million postings and 256 MiB logical payload;
each seed/packing composition has an eight-million aggregate posting-visit bound.
Identity is validated from the selected Query manifest even on cache hits.
Source generation, repository/worktree checks and historical warning behavior
remain the existing host/compiler's responsibility.

Compose at most 32 lexical seeds, cap their ordered prefix to the caller's
max_candidates (ranked selection requires seeds to fit), then select through the existing
ranked seed selection, and compose a complete discovery-balanced packing plan.
Use the caller's max_hops and max_candidates during selection; do not substitute
the diagnostic's two-hop/64-candidate settings. Original seeds for packing are
the intersection of initial seeds and selected IDs, since a small caller limit
can omit some seeds. Pack with the ranked-plan compiler at zero additional hops
and the caller's exact token budget. No oracle scan occurs on this host path.

Requests with explicit seeds retain the current lexical-plus-explicit behavior
even when the host opts in. Do not silently reinterpret their ordering or exceed
the existing raw seed limit. Cache/index failures return errors; no fallback to
another generation or ranking policy. Queries never mutate source or canonical
state. Cold index construction remains expensive and serialized with Engine
publication; expose this limitation rather than claiming constant-time cold
requests or cache persistence.

Require cache sharing/invalidation/retry/budget tests and current/historical
host fixtures for caller bounds, explicit-seed preservation, source validation,
repeat use and publication refresh. Contributor CI and representative host-path
quality/latency evidence remain required. ADR-0296 diagnostic success is not
substituted for these production-path checks.

The opt-in host path and clone-shared cache are implemented. All 27 MCP tests
and strict Query/MCP Clippy pass. Host fixtures cover one-candidate/zero-hop
requests, repeated clone reuse, unchanged explicit-seed output, empty current
and historical results, live publication cache replacement, historical changed
source omission/warnings, and return to current state. All 57 Query tests pass;
full contributor CI is running. Representative host-path quality/performance
is still required under ADR-0300; default routing remains unchanged.

Full contributor CI for ADRs 0297–0299 passes in 228.72 seconds with two
build jobs, including backend conformance and documentation. Representative
actual-host benchmark evidence remains pending.
