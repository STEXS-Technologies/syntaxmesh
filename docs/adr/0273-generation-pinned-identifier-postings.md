# ADR 0273: Generation-pinned identifier postings

Status: accepted, 2026-10-01

Host access uses the existing generation-pinned `Query` service: explicit
build and candidate methods forward the query's generation and store to this
channel. Hosts may retain an index and pass it back without receiving raw store
access or supplying a second generation coordinate. A query pinned to a later
generation rejects an older index. These methods do not enable default ranking.

Follow-up: copy Shardline's reconstruction-cache principle that entry counts
cannot bound variable-size payloads. Require an explicit build payload budget
in addition to node/posting counts. Charge each unique UTF-8 term once and
32 bytes per retained NodeId; reject overflow/exhaustion before insertion.
This is logical retained payload accounting, not allocator capacity, map
overhead, transient tokenization memory or a process RSS bound.

The two-pass reference candidate reader validates scores but scans every node
for every query. Reuse its normalization, integer rarity/coverage scoring and
stable-ID ordering in an explicitly built immutable identifier-postings index.
Use standard ordered Rust collections, with no new search service/dependency.
Keep only term-to-ID postings and population, not canonical node payloads;
hydrate selected IDs through the same pinned store generation.
Bind the derived index to repository, worktree and canonical graph root as well
as generation ID. Check that identity at query time, including empty queries;
identical opaque generation IDs in a different store must not reuse statistics.
Verification-status changes alone do not invalidate canonical graph postings.

Construction has separate node and posting budgets and fails closed rather
than returning a partial index. Queries cap posting visits independently from
the final 256-result maximum, rejecting incomplete ranking. Bind every query
to the build generation and normalization revision. No substring-search
replacement, persisted schema, default context integration or O(1) claim.
Exact-token ranking is not accepted as the production context policy: existing
whole-corpus diagnostics show semantic misses. This index is a reusable
candidate channel/reference acceleration boundary, not a retrieval-quality fix.
Incremental maintenance, durable persistence and accepted channel selection
remain separate decisions; hosts must explicitly bound index retention.

The existing 1,002-node multi-page fixture compares complete ranked payloads
and scores with the two-pass oracle, checks exact build/query budget boundaries,
absent terms, retained results after later publication and generation mismatch.
Build budgets also cap logical retained payload bytes; temporary normalization
buffers still have allocation costs. No measured speedup or representative relevance claim follows
from this fixture. The immutable in-process type uses the binary's shared
normalization revision; no serialized index compatibility is introduced.

The shared historical-node-page conformance scenario now compares indexed
candidates with the reference across InMemory, File, SQLite and Turso, before
and after later publication/deletion and after reopen/cold copies. It passes
in 9.34 seconds locally. A query fixture separately rejects a different
repository reusing the same generation ID, even when the query has no matches.

The full contributor gate before the payload follow-up passed in 217.57 seconds.
Payload fixtures accept exactly 96,208 bytes and reject one byte less; dedicated
accounting tests cover repeated-term charging and arithmetic overflow. All 37
query tests, strict Query/Turso Clippy and expanded node-page conformance pass
after the follow-up (the latter in 9.09 seconds).

The pinned Query build/read methods now pass the same 1,002-node oracle fixture;
the later-generation Query explicitly rejects the retained older index. All 37
query tests and strict all-target/all-feature Query Clippy pass after the service
methods are added. Hosts can now reuse an explicitly retained channel without
adapter-private access; default context behavior remains unchanged.

The existing sibling-repository MCP evaluator now has an opt-in
`SYNTAXMESH_CONTEXT_IDENTIFIER_INDEX_PROBE=1` (with `SYNTAXMESH_CONTEXT_CORPUS_PROBE=1`).
It builds one shared immutable index for the complete pinned corpus, reuses it
across all queries, rejects any candidate/payload/score difference from the
two-pass oracle and records build/query/oracle elapsed microseconds. Count and
payload limits remain explicit (one million nodes, eight million postings,
256 MiB logical payload). Natural-language acceptance is unchanged. Strict
MCP Clippy passes; whole-corpus execution is pending, not a performance claim.

Whole-Sim run `context-20261001T153125.620176Z-uncommitted` completes with
all five indexed results equal to the oracle (2,572 supported files; 87,486
nodes). Build time is 9.173421 s. Indexed/oracle query seconds are TypeScript
2.934720/19.281775, Python 4.083518/19.591089, JavaScript
0.292825/21.581165, Bash 1.176904/20.156108, Quickstart
1.000938/19.138344. This is one sequential local run, not an SLA or controlled
cross-machine benchmark. Candidate hydration remains costly and requires
inspection. Natural-language acceptance still fails TypeScript/Python/Bash;
the accelerated exact-token channel is not an accepted production policy.
