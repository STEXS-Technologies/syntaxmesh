# ADR-0297: Reuse a bounded generation-scoped planner cache in the MCP host

Status: accepted for private cache implementation; context routing unchanged.

ADR-0296 verifies indexed composition, but rebuilding its complete-generation
index for every context request would defeat the intended amortized lookup.
Reuse Shardline's `shardline-cache` memory adapter principles: scoped immutable
identity, explicit retention bounds, and completed-value publication only.
Do not import that project's reconstruction protocol or Redis/TTL machinery.

Implement a private MCP host cache shared by host clones, retaining at most one
planner-capable `GenerationIdentifierIndex`. Identity includes repository,
worktree, generation and canonical graph root. Index construction retains the
existing node/posting/logical-payload budgets. Cached values are disposable;
never persist them as canonical state or serialize internal service handoffs.

Lookup/build occurs inside the existing blocking query operation while the
Engine ownership mutex is held. Reuse this serialization to prevent concurrent
duplicate builds rather than adding async loader latches, heartbeats or task
supervision. Cache mutex lock ordering is Engine then cache. No public cache
operation acquires these locks in reverse order.

On identity change, release the old retained entry before building the next one
so cache-owned retention is bounded to one index, including construction. Failed
builds leave the cache empty and return the original error; do not retain partial
indexes, negative-cache failures, or silently substitute the old generation.
Every hit still validates the selected Query manifest. This is a logical retained
index bound, not a total allocator/RSS guarantee.

Keep implementation and substantial tests in focused sibling/child modules.
Require same-generation reuse, generation/root/scope invalidation, failed-build
retry, clone sharing and budget enforcement coverage. This decision does not
change ContextArgs, explicit seed semantics, traversal limits, historical source
verification, tokenizer packing or default retrieval. Routing will be a separate
decision after this cache boundary is verified.
