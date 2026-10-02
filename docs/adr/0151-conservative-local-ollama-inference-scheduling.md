# ADR-0151: Conservative local Ollama inference scheduling

- Status: accepted
- Date: 2026-09-30

## Decision

Keep `index --semantic` as the initial/incremental command and the existing
explicit cross-document option. Borrow Graphify's conservative local scheduling:
loopback endpoints on Ollama's standard port 11434 execute one prompt group at
a time. Other endpoints retain the existing four-worker bound. This is a host
heuristic, not provider detection: a custom-port Ollama deployment retains four
workers, and another service on local port 11434 is conservatively serialized.

Preserve bounded multi-request packing, adaptive splitting, durable Penelope
completion reuse, exact source validation, atomic publication, and opt-in
StateChronicle verification. Scheduling does not change prompt inputs or cache
identity. No model startup, download, new runtime, or public DTO is introduced.

Upstream research: Graphify's v8 `graphify/llm.py`,
`extract_corpus_parallel`, defaults to directory-local token-bounded batches,
four workers, adaptive truncation splitting, and immediate completion caching;
its Ollama backend defaults to one worker to reduce GPU pressure. Its skill
path can instead batch 20–25 files through host agents. It is not simply one
uncached AI invocation per document. See
https://github.com/Graphify-Labs/graphify/blob/v8/graphify/llm.py.

## Verification and remaining work

Test standard local IPv4/IPv6/localhost endpoints and unchanged custom/remote
limits without making HTTP calls. Existing CLI fixtures must retain cache and
publication behavior. This safeguard is not measured speed superiority.
Benchmark cold/warm extraction and source-grounded precision before increasing
local concurrency or adding entity-alias/topic-selection machinery.

The endpoint-policy regression passes for default, IPv4, IPv6, localhost,
custom-port, and remote endpoints. Full `cargo make ci` passes, including strict
Clippy, cache/recovery fixtures, and verified-history publication tests. Local
Ollama was unavailable, so no live throughput or extraction-quality result is
claimed.
