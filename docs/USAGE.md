# SyntaxMesh

SyntaxMesh is a local-first, Rust-native source and system intelligence engine. Its v0 vertical slice incrementally records typed engineering facts with stable identities and provenance, then serves search, graph navigation, impact analysis, and evidence-backed context to people and tools.

This repository is at the **v0 first-vertical-slice** stage. The pure model, interchange, runtime observation, extension and store-port contracts, deterministic InMemory/File/SQLite reference backends, Turso WAL adapter, runtime-neutral Rust engine, and starter source facts for Rust, Python, TypeScript/JavaScript, Bash, Markdown, and text compile and pass tests; supported source inputs are statically analyzed and their runtimes are not launched. The conservative resolver retains unresolved and ambiguous references explicitly. Penelope-backed indexing survives restart and recovers interrupted publication; StateChronicle generation verification is available as an opt-in. Shared differential scenarios cover InMemory, File, SQLite, and Turso. Operator-facing read-only workflow counts, rejection details, integrity checks, and freshness reporting are implemented. A typed Arrow fact-history feed and restartable Parquet page-partition exporter are implemented; the shared conformance fixture verifies byte-identical artifacts across InMemory, File, SQLite, and Turso, and the explicit Parquet audit checks all committed partitions. Penelope journals and retries explicit analytical exports. The separate Rust `syntaxmesh-analytics-duckdb` adapter supports bounded chronological queries over verified Parquet, explicit database migration, verified staged rebuilds, and cursor/watermark-atomic incremental synchronization; injected transaction failure verifies row/cursor rollback. Penelope-managed sync/rebuild orchestration is available behind the opt-in `analytics-duckdb` feature; it is not pulled into Engine consumers by default. Broader graph-metric/cross-snapshot queries, language semantics, representative-repository quality evidence, and remaining extension gates are open. NDJSON is an explicit graph/query export format only, never canonical storage or an internal service handoff. See [the v0 architecture plan](docs/V0_ARCHITECTURE_PLAN.md).

## Source documents

- [SyntaxMesh TODO / Source of Truth v5](docs/syntaxmesh-todo-extensible-opensource-v5.md) defines the product, architecture, and longer roadmap.
- [Change Engine Source of Truth v1](docs/syntaxmesh-change-engine-source-of-truth.md) defines a separate future project that will plan, apply, and verify changes using SyntaxMesh evidence.

The plan is an executable interpretation of those documents. Where a detail is still undecided, it is recorded as a decision gate rather than silently chosen.

## External extension import

External producers can generate one `SMEX` frame using the existing SDK
`FactBatch` schema and the framing defined in [ADR-0173](docs/adr/0173-bounded-extension-ipc-frames.md).
Import it into an already indexed store with an independently operator-authorized
manifest:

```sh
syntaxmesh ingest-extension <snapshot> <grant.json> <batch.frame> <run-id> <generation-id>
syntaxmesh ingest-extension-turso <database> <grant.json> <batch.frame> <run-id> <generation-id>
```

IDs are 64-character hexadecimal identifiers. The grant is regular JSON, limited
to 64 KiB; the frame is limited to a 4 MiB payload. Both inputs must be regular
files managed by the operator, not a concurrently replaced untrusted pathname.
Validation and authorization happen before opening the store. The existing Engine
workflow publishes accepted facts and returns one JSON receipt. Add a final
`--verify` on each import to maintain an already verified StateChronicle chain;
it does not retroactively verify an unverified base. Verification failures can
occur after canonical publication: inspect status/history before retrying.
This command
does not supervise producers, authenticate network peers, or launch other runtimes.
If stdout fails after publication, inspect the store before retrying.
See [ADR-0175](docs/adr/0175-cli-authorized-extension-import.md).

The out-of-tree fixture can produce a sample frame without Engine/store dependencies:

```sh
cargo run --quiet --locked --manifest-path fixtures/out-of-tree-extension/Cargo.toml --no-default-features -- --emit-frame > batch.frame
```

`cargo make extension-fixture` checks independent producer import and partial
producer-failure isolation on File and Turso, alongside embedded SDK conformance.

CLI watch/watch-turso, index/index-turso, ingest-extension/ingest-extension-turso, and
sqlite-migrate/turso-migrate hold the same persistent .syntaxmesh-owner.lock
sidecar lease through their mutation and output. The reusable host-only
`syntaxmesh-ownership-host::WriterLease` provides the same guard for other hosts
([ADR-0193](docs/adr/0193-reusable-host-writer-lease.md)). A competing cooperating writer
fails fast; the operating system releases ownership on process exit. Extension
inputs are validated before acquisition. Do not delete a live sidecar. This is
advisory coordination for these commands, not complete daemon ownership or
coordination of other mutation APIs
([ADR-0184](docs/adr/0184-cli-index-ownership-lease.md),
[ADR-0185](docs/adr/0185-cli-publication-and-migration-ownership.md)).

Direct CLI Turso reads also retain this lease through query and output
([ADR-0234](docs/adr/0234-leased-unattached-turso-reads.md)). Commands without
daemon attachment reject an active cooperating owner before opening the database;
stop that owner before running them. Attached commands keep discovery-first
operation and acquire the lease only for absent-owner embedded fallback.

Host callers can use `is_writer_active` to observe the same lock for an existing
store without creating files or opening its database. This is not a retained
lease or daemon endpoint discovery; direct fallback still needs ownership across
store use ([ADR-0215](docs/adr/0215-noncreating-writer-ownership-probe.md)).

## Watch local source changes

Keep the store outside the source root. For a File snapshot:

```sh
syntaxmesh watch /path/to/source /path/to/state/graph.snapshot --verify
```

For Turso, migrate first, then use `watch-turso` with the same paths. Watch uses
the existing index options, including opt-in semantic extraction/model selection.
It indexes initially, debounces events (100ms quiet, 1s maximum), and reconciles
inventory every 30 seconds. `--reconcile-ms <positive-ms>` changes that interval;
`--watch-duration-ms <positive-ms>` provides an optional timed stop.
`--poll-only` bypasses native notifications and uses the same reconciliation
interval, useful where event delivery is unavailable; it reads the source
inventory each interval and is not silently enabled on native errors
([ADR-0191](docs/adr/0191-explicit-polling-watch-fallback.md)). Ctrl-C and
Unix termination signals stop after an active pass finishes, with no bounded
shutdown deadline. Indexing/backend errors fail the command; restart for recovery.
Publication reconciliation runs before source-fingerprint no-op decisions, so
unchanged sources cannot bypass a failing publication recovery check
([ADR-0190](docs/adr/0190-recovery-before-cli-noop-indexing.md)). Semantic jobs
still require their configured provider and semantic execution path.

File reads can run alongside watch. Do not run separate Turso readers/query hosts
against an active watch writer: current backend locks can reject an open. This
is not the complete daemon/IPC or concurrent-serving implementation
([ADR-0189](docs/adr/0189-cli-watch-engine-integration.md)).

## Shared-owner daemon (initial structural slice)

`integrity-turso` attaches to GET `/api/v1/backend-integrity` and preserves the
backend-specific report and failure exit behavior. This inspects the durable
database representation, distinct from logical graph integrity in `status-turso`.
It reuses the same owned backend, performs no workflow recovery, and retains a
lease for no-owner fallback. It is an explicit potentially expensive check,
not a health probe ([ADR-0233](docs/adr/0233-owned-backend-integrity-read.md)).

`status-turso` attaches to GET `/api/v1/status`, preserving logical integrity,
workflow counts, output, and freshness/exit behavior. With a source root it
compares a local scan against the daemon's generation-consistent indexed file
inventory. This is a full graph integrity check, not a cheap health probe;
each response is limited to 4 MiB and slow checks can exceed the client timeout
([ADR-0230](docs/adr/0230-cli-status-daemon-attachment.md)). CLI freshness now
collects generation-pinned pages from `/api/v1/files`, removing the single-response
inventory ceiling without bounding total CLI memory/time. A single oversized
file fact still fails explicitly
([ADR-0232](docs/adr/0232-paged-status-inventory-attachment.md)).

GET `/api/v1/workflow-rejections?limit=20` exposes durable terminal rejection
reasons through the existing Engine; `after_run=<64-hex-run-id>` continues
exclusively. The limit is 1–100. This read does not run recovery or publish facts.
Its generation is an observation label, not a journal snapshot pin
([ADR-0227](docs/adr/0227-read-only-workflow-rejection-http.md)).
`workflow-rejections-turso` attaches automatically, preserves its existing
100-item cap and exclusive run cursor, and retains a lease for no-owner fallback
([ADR-0228](docs/adr/0228-cli-workflow-rejection-daemon-attachment.md)).

Migrate a Turso database explicitly, keep it outside the source root, then run:

```sh
syntaxmesh turso-migrate /path/to/state/graph.db
cargo run -p syntaxmesh-daemon --bin syntaxmeshd -- /path/to/source /path/to/state/graph.db 127.0.0.1:7331
```

The daemon owns one Engine and writer lease, watches all supported source types,
and serves the existing HTTP routes with live current-generation reads and
retained historical selection. Project `syntaxmesh.toml` controls resolvers and
verification; `--verify` also enables verification. Project configuration reloads
before each reconciliation pass. Resolver changes take effect without reopening
the database; deletion restores defaults. Invalid configuration stops the daemon.
Verification changes affect future publications only, with `--verify` retaining
precedence. See [ADR-0257](docs/adr/0257-daemon-project-policy-reload.md).
Reenabling verification after unverified publications does not bridge that gap:
the next publication may commit canonically, fail verification, and stop the
daemon. Inspect stored status/history before retrying; earlier verified evidence
is retained.
`--poll-only`, `--reconcile-ms`, and
`--watch-duration-ms` use the existing watch policies. Optional
`--context-tokenizer cl100k_base` (or `o200k_base`) enables the context route.
Add `--mcp --context-tokenizer cl100k_base` to serve Streamable HTTP MCP at
`http://127.0.0.1:7331/mcp` on the same listener and Engine. The SDK manages
sessions; requests share the HTTP boundary checks and have a 64 KiB body limit.
See [ADR-0214](docs/adr/0214-daemon-streamable-http-mcp.md).
Readiness is reported only after native registration and initial reconciliation.
The daemon then atomically publishes a canonical-store endpoint record under its
writer lease. Clients can discover it with `OwnerEndpoint::discover`; attached
requests must send `x-syntaxmesh-owner-instance` and verify the matching response
header. Stale or duplicate request guards return 412 before route dispatch.
The identifier protects against accidental stale-instance use, not malicious
listeners or unauthorized clients. `search-turso`, `node-turso`, and
`node-at-turso`, and `neighbors-turso` now attach automatically and preserve their tab-separated output;
`neighbors-at-turso` also attaches, preserving temporal export records, explicit
generation/direction, edge continuation, and limits up to 1,000 across HTTP pages
([ADR-0224](docs/adr/0224-cli-historical-neighbor-daemon-attachment.md)).
`resolution-diagnostics-turso` also attaches and preserves its NDJSON export.
It scans historical nodes in bounded HTTP pages, including pages with no matches,
and pins generation/repository throughout the command. This is not a dedicated
diagnostic index or a bound on total export memory/time
([ADR-0226](docs/adr/0226-bounded-diagnostic-query-and-attachment.md)).
Explicit historical node selection remains pinned. Active-owner errors never fall back to a
second database connection. When ownership is absent it retains a lease through
embedded querying. Other CLI commands are not yet attached.
See [ADR-0220](docs/adr/0220-cli-search-daemon-attachment.md).
Node reads reuse that transport ([ADR-0221](docs/adr/0221-cli-node-daemon-attachment.md)).
Current outgoing neighbors read all pages pinned to the first generation
([ADR-0222](docs/adr/0222-cli-current-neighbor-daemon-attachment.md)).
See [ADR-0219](docs/adr/0219-daemon-instance-bound-http-requests.md).

Ctrl-C and Unix termination signals finish the current pass, stop serving, and
release ownership after joining the writer. There is no bounded shutdown deadline.
A watch failure stops serving and exits with an error. Reads and publication
serialize on the shared Engine. Local IPC, CLI attachment, git monitoring,
semantic AI scheduling, analytics scheduling, and mid-publication crash/socket lifecycle coverage
remain open; this is not complete Phase 6. See
[ADR-0211](docs/adr/0211-initial-shared-owner-daemon-host.md).

Process tests additionally verify abrupt death after committed publication,
offline edit reconciliation on restart, StateChronicle history retention, and
watch failure followed by repaired-source no-op restart
([ADR-0212](docs/adr/0212-daemon-death-and-watch-failure-evidence.md)). They do not
inject process death inside every publication step.

## Read-only HTTP host

Retained fact history is available as a global paged feed:

```sh
curl 'http://127.0.0.1:8787/api/v1/fact-history?limit=100'
```

Use the host's actual bound address. The response labels its generation and
repository/worktree, with typed temporal records and sequence bounds. If
`data.next_cursor` is non-null, send its `generation`, `after_kind`, `after_id`,
and `after_valid_from` fields as query parameters on the next request, retaining
the same `limit`. A null cursor ends the scan. Explicit `generation=<hex>` starts
at retained history; omitting it selects the current generation once, then the
returned cursor pins subsequent pages. Limits are 1–100; JSON responses are
capped at 4 MiB, and a single oversized fact can cause HTTP 413. This is not an
identity-filtered replacement for CLI `history`/`fact-history`.
See [ADR-0237](docs/adr/0237-owned-fact-history-http-pages.md).

Serve an already migrated and indexed Turso database locally:

```sh
cargo run -p syntaxmesh-http -- .syntaxmesh/graph.db 127.0.0.1:7331
```

GET `/api/v1/search?text=Penelope&limit=20`, `/api/v1/nodes/<64-hex-id>`,
and `/api/v1/generation` return versioned JSON. Add
`generation=<64-hex-generation>` to search or node requests for historical
results ([ADR-0197](docs/adr/0197-generation-selected-http-search.md)).
`/healthz` reports process liveness. The host pins its startup
generation and returns 409 if a writer advances it; restart to select the new
graph. It does not watch, index, or migrate the database.

Trusted Rust hosts can instead construct `SyntaxMeshHttp::from_shared_engine`
with a shared Turso Engine lock and explicit repository/worktree scope. Any
Send-capable language extractor is supported, including the SDK's
`SendCompositeExtractor`; the host does not duplicate language dispatch
([ADR-0201](docs/adr/0201-generic-shared-http-extractors.md)). Default
reads and readiness then follow completed publications without restarting HTTP;
explicit historical reads remain pinned. The owner must hold a writer lease,
run durable recovery, and retain the Engine outside the async runtime until
shutdown. Reads and indexing serialize on that lock; this is not a standalone
daemon or concurrent read/write performance guarantee
([ADR-0199](docs/adr/0199-shared-engine-live-http-host.md)).

GET /readyz reports the startup generation and durable workflow recovery counts.
It returns 503 for pending prepared operations, a stale generation, admission
exhaustion, shutdown, or diagnostic failure. It never performs recovery or full
graph integrity validation; terminal rejected operations are reported without
blocking readiness. Workflow diagnostics enumerate operation records, so this
is not a constant-time probe ([ADR-0183](docs/adr/0183-http-readiness.md)).

Use the literal bound address in requests. Nonmatching Host headers, Origin
headers, and cross-site browser requests are rejected. This is a local trusted
process boundary, not authenticated multi-user or remote serving. Query admission
is bounded; Ctrl-C drains requests, without a promised shutdown deadline.
The shutdown signal closes query admission for every host clone; accepted
blocking workers are not cancelled. Reopen the host to serve again
([ADR-0182](docs/adr/0182-http-shutdown-admission.md)).
See [ADR-0177](docs/adr/0177-read-only-loopback-http-host.md).

Page graph connections with
`GET /api/v1/nodes/<id>/neighbors?direction=outgoing&limit=20`.
Use `direction=incoming` for inbound connections and add `generation=<generation-id>`
for retained history. Responses include typed edge/node pairs, `has_more`, and
an opaque `next_cursor`; pass that token unchanged as `cursor` with the same
endpoint, direction, and generation. Page size may change between requests.
Both current and historical pages use the existing incidence index rather than
replaying deltas or loading a complete graph. Missing endpoints return 404;
invalid or mismatched continuations return 400.
See [ADR-0178](docs/adr/0178-http-historical-neighbor-pages.md).

Use `GET /api/v1/nodes/<id>/neighborhood?max_hops=2` for a bounded weakly
connected multi-hop result. Optional caps are `max_nodes`, `max_edges`,
`max_scanned_edges`, and `max_result_bytes`; `generation` selects retained
history. Results retain edge direction and node/edge discovery depth, and report
examined incidences and `truncated`. Successful JSON bodies honor the byte cap;
413 means the metadata/seed cannot fit. Error responses are not subject to that
successful-body cap. These results are not pageable and do not imply complete
impact when truncated. The shared query algorithm remains authoritative; HTTP
budgets its own JSON without constructing NDJSON export records.
See [ADR-0179](docs/adr/0179-http-bounded-historical-neighborhood.md).

Enable source-verified context retrieval with an explicit root and tokenizer:

```sh
cargo run -p syntaxmesh-http -- .syntaxmesh/graph.db 127.0.0.1:7331 --context . o200k_base
```

POST `/api/v1/context` with JSON such as
`{"query":"architecture","token_budget":8192}`. Optional fields are `generation`,
`seed_nodes` (64-hex IDs), `max_hops` (default 1), and `max_candidates` (default
64). The response is a schema-2 ContextPack directly, with exact token accounting
for its whole compact JSON body. Supported tokenizers are `cl100k_base` and
`o200k_base`; this endpoint does not invoke AI. Requests are capped at 64 KiB,
4096 query bytes, 64 seeds, 32768 tokens, 8 hops, and 256 candidates. Without
source configuration it returns 503; a budget too small for the minimum pack
returns 413. Historical source text is returned only when available bytes match
the indexed hash; changed or missing evidence retains compiler warnings and
omissions. See [ADR-0181](docs/adr/0181-http-context-retrieval.md).

## One-command documentation graph

Use a locally installed, signed-in Codex harness with the supplied
[command configuration](fixtures/codex-semantic-command.json). It selects
`gpt-6-luna` by default with medium reasoning; SyntaxMesh sends the extraction prompt on
stdin and validates the final claims JSON from stdout. Codex owns authentication
and inference—SyntaxMesh does not implement a Codex API or consume its JSONL
event stream.

From this workspace, index the current project in one command:

```sh
cargo run -p syntaxmesh-cli -- index --semantic --semantic-command @fixtures/codex-semantic-command.json
```

For an installed CLI, use `syntaxmesh index --semantic gpt-6-luna
--semantic-command @/path/to/codex-semantic-command.json`. An inline JSON argv
array works too. Add `--semantic-cross-document` for bounded joint-source
claims. Re-run the same command for updates and interrupted-work recovery.
Command transport reuses the existing prompt packing, Penelope completion
cache, source-evidence validation, and atomic historical publication
([ADR-0169](docs/adr/0169-local-command-semantic-harness.md)). It runs one bounded
prompt group at a time from a temporary working directory. The command file
controls settings and passes the selected model through its `{model}` argv element.
Use `--semantic <model-name>` to override the default without editing that file.
Literal commands without the placeholder must have a matching model label.
See [ADR-0170](docs/adr/0170-selectable-command-semantic-model.md). Change
`--semantic-model-revision` when a model alias or external harness configuration
changes. No immutable remote model revision is automatically discovered.

Only configure trusted executables. Local harness execution can still send
source text to a remote model; it is not offline inference. Authentication and
network policy belong to the harness, so command mode rejects the HTTP endpoint,
API-key, and `--allow-network` options. `--semantic-offline` with an explicit
revision reuses completed cache entries without launching the harness; a miss
fails. Token usage is explicitly unavailable in final-JSON-only command mode.

The HTTP-compatible adapter remains an alternative. For that path:

With Ollama already running and `qwen3:latest` installed, run from the project
you want to index:

```sh
syntaxmesh index --semantic
```

From this Rust workspace without installing the CLI:

```sh
cargo run -p syntaxmesh-cli -- index --semantic
```

The same command handles the initial build and incremental updates: deterministic
source indexing, cached AI document claims, bounded parallel prompts, automatic
multi-document truncation recovery, and historical graph publication. Omit
`--semantic` for provider-free indexing. The command does not install or start a
model. To add bounded joint-source claims across documents in the same command:

```sh
syntaxmesh index --semantic --semantic-cross-document
```

Document extraction and compound requests have independent durable cache units.
Both layers merge before one semantic publication. This adds inference work on
cold inputs; it is not a measured speed or quality improvement over Graphify.
Real-model quality, corpus-wide synthesis, and entity aliases remain open gates.

Use `--semantic` for the inexpensive baseline: document-local claims are cached
independently even when several documents share an HTTP prompt. Add
`--semantic-cross-document` when claims need evidence from multiple documents;
those compound requests reuse the document cache but add their own inference
cost. Re-run the same command after edits or interruptions: successful work is
durable, only cache misses need inference, and a failed semantic run does not
publish a partially enriched graph. Exact supporting quotes are checked, but
quote matching alone does not prove that an AI claim follows from its evidence.

The shared v4 extraction instructions request reusable component names, separate
responsibility/rationale claims, and complete premise/consequence quotations
([ADR-0172](docs/adr/0172-reusable-semantic-concepts-and-complete-evidence.md)).
Changing that policy invalidates inference caches while preserving accepted
historical generations. These are model instructions, not deterministic alias
normalization or an entailment verifier; naming and relation drift remain possible.

Standard loopback Ollama inference uses one prompt at a time to limit local
GPU pressure; other endpoints allow up to four concurrent prompts. Each prompt
still packs bounded pending document requests, so serial local inference does
not mean one HTTP request per file.

To evaluate the locally installed Codex harness with the default model, run:

```sh
SYNTAXMESH_SEMANTIC_EVAL_COMMAND=@fixtures/codex-semantic-command.json cargo make evaluate-semantic-provider
```

Set `SYNTAXMESH_SEMANTIC_EVAL_MODEL` to choose another model and
`SYNTAXMESH_SEMANTIC_EVAL_REVISION` to assert a specific model/configuration
revision. Command mode defaults to `gpt-6-luna`; the supplied harness configuration
uses medium reasoning. The cargo-make task preserves its workspace working
directory for relative command-file paths instead of the test process's crate
directory. Configure only trusted commands; inference can be remote.

For an already running HTTP-compatible provider, use:

```sh
SYNTAXMESH_SEMANTIC_EVAL_ENDPOINT=http://127.0.0.1:11434/v1 cargo make evaluate-semantic-provider
```

The opt-in evaluator retains initial, cached-repeat, one-document-edit, and
edited-repeat results with source quotations for review. Set
`SYNTAXMESH_SEMANTIC_EVAL_CROSS_DOCUMENT=true` to evaluate the joint-source layer
as well. Select exactly one endpoint or command; no model is installed or started
automatically. The retained report identifies transport without recording argv,
endpoints, or credentials. Command token usage is explicitly unavailable. Mock
tests do not establish model quality or comparative speed.

SyntaxMesh itself is implemented and run in Rust. Rust, TypeScript/JavaScript,
Python, and Bash are analyzed source formats—not runtimes that SyntaxMesh
launches—and Markdown/plain-text documentation is indexed as well. The Node
and Python resolution profiles are static Rust analysis, not runtime
dependencies. Other implementation runtimes or analyzed source languages are
out of scope unless explicitly approved ([ADR-0078](docs/adr/0078-rust-only-execution-and-input-language-scope.md)).

## Architecture at a glance

```text
source / git / docs / runtime observations
                 |
                 v
       embeddable SyntaxMesh engine
          |        |         |
       store    indexer   extension SDK
          |        |         |
  Turso / SQLite resolver   adapters/probes

full engine durable workflows -> Penelope (required)
accepted graph generations  -> StateChronicle adapter (present from day one;
                               verified-history recording opt-in)
analytics/history            -> DuckDB downstream of canonical state
```

The optional [`syntaxmesh-analytics-duckdb`](crates/syntaxmesh-analytics-duckdb/src/lib.rs)
adapter embeds DuckDB through its Rust binding and reads only complete,
integrity-verified Parquet exports. It is not used by core, live graph queries,
MCP, or any analyzed-language runtime. Its current typed query counts fact
versions by valid-from generation with a bounded result size.

The core and thin runtime protocols stay free of host, database, and workflow dependencies. The daemon, CLI, HTTP, and MCP surfaces call the same engine API.

The separate `syntaxmesh-mcp` executable provides a read-only stdio MCP surface
over Turso-backed generation queries, including a repository-scoped, exact-token
context tool. Protocol, filesystem, tokenizer, and Tokio dependencies remain in
that host crate; graph selection and evidence packing stay in the runtime-neutral
Rust query crate. See the [operations runbook](docs/OPERATIONS.md) for setup,
tokenizer selection, and generation-pinning behavior.
The context tool accepts an optional retained `generation` ID within the same
repository/worktree. Omitted selection uses the startup generation. Historical
graph evidence is available without indexing replay; changed filesystem bytes
are excluded from source snippets and reported as `stale_source`. This does not
archive historical source files or bypass stale-host restart requirements.

Add `--source-content-context` to the standalone MCP launch command to opt into
indexed exact/stemmed source-content selection:

```sh
syntaxmesh-mcp graph.db /path/to/source cl100k_base --source-content-context
```

Add `--lexical-first-context` to prioritize original lexical seeds before graph
discoveries during indexed packing. It composes with `--source-content-context`;
balanced packing remains default. This is an explicit ordering preference, not
a guarantee of better retrieval for every query. Explicit seed IDs bypass it.

This excludes reserved processing metadata from lexical candidates before limits,
while raw search and explicit seed IDs retain their original behavior. Cold index
construction scans the selected generation and can be expensive; subsequent
requests reuse its policy-scoped cache. The default remains unchanged. This is
not a whole-repository relevance or performance guarantee
([ADR-0327](docs/adr/0327-source-content-retrieval-and-processing-evidence.md)).

Embedded owners can instead construct `SyntaxMeshMcp::from_shared_engine` with
their existing Send-capable Engine, exact scope, source root, and tokenizer.
Shared reads refresh after publication without a second Turso connection;
standalone `open` remains startup-pinned. Retain the Engine outside the async
runtime until shutdown. The daemon uses this composition for its optional `/mcp`
transport ([ADR-0213](docs/adr/0213-shared-engine-live-mcp-host.md),
[ADR-0214](docs/adr/0214-daemon-streamable-http-mcp.md)).

Graph traversals bind to one generation. The file/reference store uses
generation-scoped adjacency indexes; Turso exact node/symbol, file-ownership,
node-kind (including module-resolution inventory), and edge-direction lookups use indexed SQL. Turso rows are authoritative and
startup validates indexed projections and streams the graph root without
restoring a whole graph into memory. It validates the latest history entry;
the explicit backend integrity check decodes all retained history payloads.
Delta writes touch changed rows only;
canonical-root verification still scans the ordered facts. Substring search
currently scans indexed names in SQL and remains a scaling limitation.
The pinned Turso crate's optional native FTS feature is token-oriented, so it
cannot replace SyntaxMesh's existing arbitrary substring semantics; a Rust
capability probe is documented in the [v0 architecture plan](docs/V0_ARCHITECTURE_PLAN.md).


A source-grounded semantic-facts path is now available through the separate
Rust `syntaxmesh-semantic` contract and the Engine's explicit opt-in enrichment
method. The Engine assembles requests from generation-pinned `DocumentChunk`
facts either per file or as deterministic, byte/chunk-bounded batches across a
whole generation; batches carry authored section-heading paths without
reparsing source or exposing physical file paths. A host supplies the provider;
exact-quote claims become generation-scoped graph facts with
`SemanticInference` provenance. Penelope journals
prepared jobs and reuses validated output by content plus provider/model/
prompt/configuration identity through the store's existing durable-record CAS
port. An opt-in CLI host adapter now runs bounded semantic batches through a
local OpenAI-compatible model in the same `index` command. Penelope caches
document-scoped requests independently, while the CLI combines up to 22
pending requests per prompt and runs up to four prompts in flight. The default
endpoint is loopback Ollama; a remote HTTPS endpoint requires
`--allow-network`.
Automatic HTTP redirects are disabled, and loopback providers bypass
environment-configured proxies
([ADR-0128](docs/adr/0128-semantic-http-endpoint-policy.md)).
Exact-quote validation and provider-identity cache keys guard publication,
which replaces the current semantic layer in one generation
while retaining its history.
The provider receives explicit independent request groups in each packed
prompt, so it can see which chunks may jointly support one cached claim
([ADR-0127](docs/adr/0127-explicit-semantic-prompt-request-boundaries.md)).
Embedded callers can use `enrich_document_semantic_outputs_many` to obtain the
validated, Penelope-cached claims and their complete source request, compose
layers with `SemanticOutput::merge`, then ground the merged output before atomic
publication. This reuses the existing cache format; it does not enable a second
CLI synthesis phase by itself
([ADR-0145](docs/adr/0145-validated-semantic-output-composition.md)).
Deterministic indexing remains offline and
provider-free unless `--semantic` is supplied. From the project root, run
`syntaxmesh index --semantic`; the root defaults to the current directory and
the snapshot to `.syntaxmesh/index.snapshot`. This runs deterministic indexing
and source-grounded AI enrichment in one command. AI work is cached per
document, packed into bounded multi-document prompts, and sent in parallel;
unchanged documents are reused, while validated semantic facts publish
atomically with history retained. Use `--semantic <model>` to select another
installed model, or `syntaxmesh index <root> <snapshot> --semantic` for explicit
paths. The CLI does not download weights or start Ollama.
Explicitly truncated multi-document responses are automatically retried as
smaller prompts, with a three-level split limit and no extra flags. Invalid
evidence is not retried. Validated document results from successful recovery
leaves are durably cached even if a sibling fails; the next invocation sends
only remaining documents, while graph publication stays atomic
([ADR-0143](docs/adr/0143-cache-successful-semantic-recovery-leaves.md)).
Each claim must still be supported within one cached
document request: prompt batching does not imply cross-document synthesis
([ADR-0142](docs/adr/0142-bounded-semantic-truncation-recovery.md)).
Local runs discover Ollama's reported model digest before cache lookup and
recheck it after inference and before semantic publication. Changed digests
invalidate old cached output. Providers without Ollama metadata require
`--semantic-model-revision <immutable-revision>`; this caller assertion skips
metadata checks and permits offline cache reuse. The caller must update the
asserted revision when the model changes. The command reports inference and
metadata request counts separately. These checks rely on server metadata, not
atomic model pinning or attestation
([ADR-0129](docs/adr/0129-semantic-model-revision-identity.md)).
Each run also reports provider-supplied prompt/completion/total token counts,
including inference whose results fail validation. Missing or malformed usage
is explicit; these are reported totals, not price estimates or durable billing
history ([ADR-0130](docs/adr/0130-host-semantic-token-usage.md)).
Use `--semantic-offline --semantic-model-revision <revision>` to reuse results
without inference or metadata HTTP calls. Keep the same endpoint, model, revision,
and credential configuration as the online run. A cache miss fails the semantic
step; deterministic indexing still proceeds. This requires a previously asserted
revision, not an automatically discovered Ollama digest
([ADR-0131](docs/adr/0131-offline-semantic-cache-execution.md)).
Graphify also uses deterministic code extraction, content-hash cache checks,
and parallel batches;
SyntaxMesh's differentiator here is its durable per-document workflow cache,
exact source evidence validation, and temporal graph publication. See
[ADR-0119](docs/adr/0119-one-command-parallel-semantic-indexing.md),
[ADR-0121](docs/adr/0121-default-local-model-for-semantic-indexing.md), and
[ADR-0124](docs/adr/0124-default-project-local-index-command.md).
The [workflow
port](crates/syntaxmesh-workflow/src/lib.rs) makes publication
and verification status explicit. The engine uses the Penelope and
StateChronicle integration crates; Penelope process events and the prepared
delta are persisted as opaque compare-and-swap records, replayed on recovery,
and reconciled against the graph generation. StateChronicle's durable,
hash-linked generation history remains explicitly opt-in. Normal verified
publication checks the current chain head; `statechronicle-verify[-turso]`
replays the full retained chain. The [operations runbook](docs/OPERATIONS.md#start-and-inspect-verified-history)
shows the end-to-end workflow and anchor-generation caveat. This is not a
signed or portable proof. The
CLI offers the reference file
store and a Turso WAL path; SQLite is available as a host-embeddable reference
backend through the shared store trait. Both durable SQL adapters use an
explicit migration lifecycle: call `migrate(path)` before `open(path)` for a
new or stale database. `open` validates but never migrates, creates schema
objects, or backfills history. The CLI exposes read-only
`*-migration-status` commands and explicit `*-migrate` upgrades. There is no
generic rollback command because some data migrations have no safe inverse.
See the [operations notes](docs/OPERATIONS.md#sqlite-schema-migrations).

Accepted graph deltas are also retained as append-only generation history,
atomically with the current projection in Turso and SQLite. Stores expose the
ordered manifest/delta history; pre-history databases start at an explicit
current-generation anchor because earlier fact versions cannot be recovered.
Durable stores maintain indexed temporal fact versions, structurally shared
content-addressed generation roots, and full checkpoints at generation 1 and
every 64 generations. Historical node lookup uses the identity/version index;
durable `GraphAt` resolves one retained root and walks only that generation's
facts, verifying the manifest root without replaying deltas or scanning prior
versions. Checkpoints support recovery, migration, and index rebuild;
InMemory/File keep replay as the reference path. Full-graph reads still cost
at least the graph output size.
The typed query service and CLI expose node `History`, `GraphAt`,
`ChangedBetween`, and a bounded acceptance-time timeline for retained
generations. The embedded engine query also exposes typed `fact_history` for
file, provenance, node, and edge payload versions; SQLite/Turso use the
identity/validity index, while the file and in-memory stores are reference
implementations. `fact_history` records generation-validity bounds plus
independent optional observation and acceptance times. It returns canonical
fact versions, not inferred consequence lineage. Use
`fact-history-turso <database> <file|provenance|node|edge> <id-hex>` (or
`fact-history` for the file store) to inspect one fact's versions; use
`fact-lineage[-turso] <store> <kind> <id>` for contiguous same-identity version
`supersedes` links. Removal followed by reintroduction is not presented as
continuity, and temporal proximity alone does not imply causation. For example,
`change-correlations[-turso] <store> <source-generation> <kind> <id> <limit>
[after-generation]` pages later events that also changed one selected fact.
This is explicitly historical correlation, not a causal/dependency claim; pass
the returned generation cursor to continue the page.
use `history-turso
<database> <node-id-hex>`, `graph-at-turso <database> <generation-id-hex>`,
or `graph-at-known-by-turso <database> <generation-id-hex> <accepted-by-ns>`,
or `changes-turso <database> <from-generation-hex> <to-generation-hex>`;
equivalent commands exist for the reference file store. Node histories expose
generation validity intervals, changes and accepted-generation ranges are
emitted as NDJSON records. Use `node-at-turso <database> <generation-id-hex>
<node-id-hex>` (or `node-at <snapshot> <generation-id-hex> <node-id-hex>`) to
read one node at a retained generation without materializing that generation's
complete graph. SQLite/Turso use the temporal identity/version index for this
point query; reference stores may use their history replay path. Use
`accepted-between-turso <database>
<from-unix-nanos> <until-unix-nanos> <limit> [after-time after-generation-id]`
(or `accepted-between` for the file store) for the half-open acceptance-time
interval. Pass the footer cursor values to fetch the next page. This lists known
generation acceptances; it does not select or reconstruct state at a calendar
time. `observed-between-turso <database> <from-unix-nanos>
<until-unix-nanos> <limit> [after-time after-kind after-fact-id
after-generation-id]` (or `observed-between` for the file store) pages through
known producer event times without selecting graph state. The source timeline
currently covers runtime observations. Consequence assertions now publish
atomically and support bounded fixed-generation endpoint lookup and weakly
connected BFS through the embedded query API and versioned CLI export. The
scoped `graph-at-known-by` query qualifies an exact modeled generation by an
engine-acceptance cutoff using an ancestry-prefix maximum; durable stores read
the prefix directly and report legacy unknown timestamps as indeterminate. It
does not select by producer observation/calendar time or replay deltas. Generic
calendar-valid-time queries, hot/warm/cold query planning, and transitive
propagation remain future work.
Serialized graph/history records carry a `query_mode`: generation-scoped
results use `historical_conclusion`, while the two timestamp listings use
`acceptance_timeline` and `observation_timeline`. The
`current_semantics_retrospective` mode is reserved for a future implementation
and is not emitted. See [ADR-0035](docs/adr/0035-explicit-temporal-query-mode.md).
Synthetic query benchmarks
now cover both retained history depth and graph output sizes through 5,000
nodes. A three-run benchmark now covers a real 696-file Shardline Rust tree,
including retained bytes and per-table accounting; an opt-in Linux syscall pass
provides a bounded write-traffic proxy. Broader repository coverage and complete
physical write-amplification measurement remain open. This is a query
foundation, not complete temporal intelligence.

Public extension fact batches publish through the engine and Penelope.
Runtime-protocol observations are retained as distinct `RuntimeObservation`
nodes with `RuntimeObserved` provenance and their versioned envelope intact;
they do not create static edges from opaque producer identifiers.
The CLI `status` command reports current generation state, read-only
prepared/completed/rejected workflow-operation counts, and logical graph-root
and fact-reference integrity checks. This is not a physical database-page
audit; the explicit `integrity` commands also check SQL pages and retained
graph checkpoints against historical roots. See the [v0 operations
runbook](docs/OPERATIONS.md).

## Local smoke path

Inspect explicit source-processing evidence at a current or retained generation:

```sh
syntaxmesh processing-coverage graph.snapshot current
syntaxmesh processing-coverage-turso graph.db <generation-id>
```

Output is one versioned JSON object with completed, syntax-failed and unclassified
file IDs. Failure entries include bounded diagnostics and a truncation flag.
Legacy files without processing evidence remain unclassified, not clean. This
read materializes the selected snapshot. Turso requires absent-owner embedded
operation; active-daemon attachment is not implemented. The command does not
enable partial ingestion. Indexing explicitly opts in with
`syntaxmesh index <root> <snapshot> --record-syntax-failures` (also supported by
`index-turso`). Partial runs report observed files and processing counts; failed
files retain diagnostics, not stale successful facts. Unchanged failed files are
retried without publishing a new generation when facts are unchanged. Embedded
callers select `SourceSyntaxPolicy::RecordFailures`. Strict remains the default;
daemon project configuration has no partial-ingestion option yet.
See [ADR-0326](docs/adr/0326-generation-scoped-source-processing-coverage.md).

Analyze directed cycle components in current or retained facts:

```sh
syntaxmesh cycles graph.snapshot current calls
syntaxmesh cycles-turso graph.db <generation-id> imports
```

Use `all` instead of `calls`/`imports` to include every accepted relation. Output
is one JSON object containing generation, relation, and stable node-ID component
arrays; self-loops count as cycles. This explicitly materializes the selected
graph, not a constant-time or bounded traversal. Turso requires absent-owner
embedded operation; daemon attachment is not implemented for this command.
See [ADR-0240](docs/adr/0240-query-and-cli-cycle-analysis.md).

The current reference path can index Rust, Python, TypeScript/JavaScript, Bash,
Markdown, and plain-text files into a restartable snapshot,
search the published generation, and export deterministic NDJSON:

```text
cargo run -p syntaxmesh-cli -- --help
cargo run -p syntaxmesh-cli -- init <source-root> --verified-history
cargo run -p syntaxmesh-cli -- index --semantic
cargo run -p syntaxmesh-cli -- index <source-root> <snapshot-file>
cargo run -p syntaxmesh-cli -- turso-migration-status <turso-database>
cargo run -p syntaxmesh-cli -- turso-migrate <turso-database>
cargo run -p syntaxmesh-cli -- index-turso <source-root> <turso-database>
cargo run -p syntaxmesh-cli -- search <snapshot-file> <text>
cargo run -p syntaxmesh-cli -- search-turso <turso-database> <text>
cargo run -p syntaxmesh-cli -- node <snapshot-file> <node-id-hex>
cargo run -p syntaxmesh-cli -- node-at <snapshot-file> <generation-id-hex> <node-id-hex>
cargo run -p syntaxmesh-cli -- history <snapshot-file> <node-id-hex>
cargo run -p syntaxmesh-cli -- fact-history <snapshot-file> <file|provenance|node|edge> <id-hex>
cargo run -p syntaxmesh-cli -- fact-lineage <snapshot-file> <file|provenance|node|edge> <id-hex>
cargo run -p syntaxmesh-cli -- change-correlations <snapshot-file> <source-generation> <file|provenance|node|edge> <id-hex> <limit> [after-generation]
cargo run -p syntaxmesh-cli -- graph-at <snapshot-file> <generation-id-hex>
cargo run -p syntaxmesh-cli -- graph-at-known-by <snapshot-file> <generation-id-hex> <accepted-by-ns>
cargo run -p syntaxmesh-cli -- changes <snapshot-file> <from-generation-hex> <to-generation-hex>
cargo run -p syntaxmesh-cli -- accepted-between <snapshot-file> <from-unix-nanos> <until-unix-nanos> <limit> [after-time after-generation-id]
cargo run -p syntaxmesh-cli -- observed-between <snapshot-file> <from-unix-nanos> <until-unix-nanos> <limit> [after-time after-kind after-fact-id after-generation-id]
cargo run -p syntaxmesh-cli -- neighbors <snapshot-file> <node-id-hex>
cargo run -p syntaxmesh-cli -- neighbors-at <snapshot-file> <generation-id-hex> <node-id-hex> <incoming|outgoing> <limit> [after-edge-id-hex]
cargo run -p syntaxmesh-cli -- neighborhood-at <snapshot-file> <generation-id-hex> <seed-id-hex[,seed-id-hex...]> <max-hops> <max-nodes> <max-edges> <max-scanned-incidences> <max-result-bytes>
cargo run -p syntaxmesh-cli -- path <snapshot-file> <start-id-hex> <target-id-hex> <max-hops>
cargo run -p syntaxmesh-cli -- subgraph <snapshot-file> <seed-id-hex> <max-hops> <max-nodes> <max-edges>
cargo run -p syntaxmesh-cli -- impact <snapshot-file> <text>
cargo run -p syntaxmesh-cli -- node-turso <turso-database> <node-id-hex>
cargo run -p syntaxmesh-cli -- node-at-turso <turso-database> <generation-id-hex> <node-id-hex>
cargo run -p syntaxmesh-cli -- resolution-diagnostics-turso <turso-database>
cargo run -p syntaxmesh-cli -- history-turso <turso-database> <node-id-hex>
cargo run -p syntaxmesh-cli -- fact-history-turso <turso-database> <file|provenance|node|edge> <id-hex>
cargo run -p syntaxmesh-cli -- fact-lineage-turso <turso-database> <file|provenance|node|edge> <id-hex>
cargo run -p syntaxmesh-cli -- change-correlations-turso <turso-database> <source-generation> <file|provenance|node|edge> <id-hex> <limit> [after-generation]
cargo run -p syntaxmesh-cli -- graph-at-turso <turso-database> <generation-id-hex>
cargo run -p syntaxmesh-cli -- graph-at-known-by-turso <turso-database> <generation-id-hex> <accepted-by-ns>
cargo run -p syntaxmesh-cli -- changes-turso <turso-database> <from-generation-hex> <to-generation-hex>
cargo run -p syntaxmesh-cli -- change-set-at-turso <turso-database> <change-set-id-hex> <generation-id-hex>
cargo run -p syntaxmesh-cli -- change-set-events-turso <turso-database> <change-set-id-hex> <generation-id-hex> <limit> [after-event-generation-hex]
cargo run -p syntaxmesh-cli -- accepted-between-turso <turso-database> <from-unix-nanos> <until-unix-nanos> <limit> [after-time after-generation-id]
cargo run -p syntaxmesh-cli -- observed-between-turso <turso-database> <from-unix-nanos> <until-unix-nanos> <limit> [after-time after-kind after-fact-id after-generation-id]
cargo run -p syntaxmesh-cli -- neighbors-turso <turso-database> <node-id-hex>
cargo run -p syntaxmesh-cli -- neighbors-at-turso <turso-database> <generation-id-hex> <node-id-hex> <incoming|outgoing> <limit> [after-edge-id-hex]
cargo run -p syntaxmesh-cli -- neighborhood-at-turso <turso-database> <generation-id-hex> <seed-id-hex[,seed-id-hex...]> <max-hops> <max-nodes> <max-edges> <max-scanned-incidences> <max-result-bytes>
cargo run -p syntaxmesh-cli -- path-turso <turso-database> <start-id-hex> <target-id-hex> <max-hops>
cargo run -p syntaxmesh-cli -- subgraph-turso <turso-database> <seed-id-hex> <max-hops> <max-nodes> <max-edges>
cargo run -p syntaxmesh-cli -- impact-turso <turso-database> <text>
cargo run -p syntaxmesh-cli -- export <snapshot-file> > graph.ndjson
cargo run -p syntaxmesh-cli -- export-turso <turso-database> > graph.ndjson
cargo run -p syntaxmesh-cli -- status <snapshot-file> [source-root]
cargo run -p syntaxmesh-cli -- status-turso <turso-database> [source-root]
cargo run -p syntaxmesh-cli -- workflow-rejections <snapshot-file> <limit> [after-run-id-hex]
cargo run -p syntaxmesh-cli -- workflow-rejections-turso <turso-database> <limit> [after-run-id-hex]
cargo run -p syntaxmesh-cli -- integrity <snapshot-file>
cargo run -p syntaxmesh-cli -- integrity-turso <turso-database>
```

The plain `index` command is the development/conformance host using the
restartable file store. The `*-turso` commands exercise the Turso-backed host
path. Turso CLI search and graph traversals use indexed SQL reads; status and
complete-generation export still use the restored generation snapshot.
StateChronicle history recording remains opt-in: set `[history] verified =
true` in the source-root `syntaxmesh.toml` (created by `init --verified-history`)
or pass `--verify` for one run. Without an enabled setting or override,
indexing remains `DURABLE` and appends no StateChronicle record.
ECMAScript package/path resolution is independently opt-in with
`[module_resolution] profile = "node"` or `syntaxmesh init
--node-module-resolution`. Python resolution is independently opt-in with
`profile = "python"` or `syntaxmesh init --python-module-resolution`; mixed
projects can use `profiles = ["node", "python"]`. Optional ordered Python
`source_roots` are repository-relative. The CLI chooses `import`/`require` package
conditions from each source occurrence and links only targets already indexed
in the repository; non-success outcomes are retained as separate typed,
provenance-backed diagnostic nodes in that generation. Inspect the current
Turso outcomes with `resolution-diagnostics-turso <database>`; graph export and
historical `GraphAt` include the same facts. The embedded Engine continues to
require explicit provider injection.
Re-running
`index` removes files and file-owned facts that disappeared from the source root.
Passing `source-root` to `status` performs a read-only inventory comparison;
the command reports added/changed/removed supported-language files and exits non-zero when the
index is stale. Without a root, freshness is reported as `not_checked`.

## Contributor checks

The workspace uses Rust 2024 and pins Rust/rustc 1.98.1. `cargo make ci` runs formatting, architecture dependency checks, compilation, strict Clippy, `cargo audit`, `cargo deny`, tests, and API documentation. The initial policy follows Shardline's Clippy deny list and dependency-source/license checks. `deny.toml` documents the temporary bincode v1 unmaintained notice exception; its durable payload format needs an ADR and compatible migration before replacement.

GitHub Actions runs that same contributor gate on Ubuntu, macOS, and Windows for pull requests and pushes; local success on one operating system does not substitute for those hosted matrix results.

For an additional hardening pass borrowed from Shardline, run
`cargo make test-overflow` to rerun workspace tests with Rust integer overflow
checks enabled, or `cargo make ci-overflow` to add that lane after standard CI.
The default `ci` task remains the faster contributor feedback path.

Run `cargo make fuzz-source-smoke` for a bounded coverage-guided parser check
borrowed from Shardline's isolated fuzz-package approach. It requires an
installed nightly toolchain and `cargo-fuzz`, defaults to 30 seconds, and accepts
`SYNTAXMESH_FUZZ_SECONDS` as a duration override. The Rust target covers the
supported language and documentation extractors; each run copies checked-in
seeds into a disposable corpus. Crash artifacts are retained under ignored
`crates/fuzz/artifacts/syntaxmesh_source_extractors/`. This campaign is separate
from stable CI; CI checks the target's formatting and workspace isolation.

Run `cargo make benchmark-temporal` for retained-history depth, graph output
size, and publication scaling across InMemory, SQLite, and Turso. It reports
fixed-result historical node, `GraphAt`, ten-generation `ChangedBetween`,
acceptance-time timeline, and observation-time timeline medians, plus full
snapshot and one/ten/100-fact publication medians for synthetic graphs up to
5,000 nodes and 4,999 edges. The task puts both database fixtures and Turso's
temporary files under ignored workspace `target/benchmark-scratch/` by default,
avoiding reliance on system `/tmp` space.

Run `SYNTAXMESH_BENCH_ROOT=/path/to/rust/repository cargo make benchmark-repository`
to exercise the full scanner → Rust extractor/resolver → Engine/Penelope
publication path on a real Rust tree with InMemory, SQLite, and Turso stores.
The benchmark reports initial indexing, a one-file incremental change, graph
size, and final persisted database bytes. The incremental edit is made only to
an in-memory source copy. Reported logical source bytes are input-size context,
not physical write-amplification measurements.

For repeatable evidence bundles, use `cargo make benchmark-repository-evidence`;
it records raw logs, host/toolchain/source metadata, each fresh-database sample,
database page/payload accounting when SQLite-compatible `dbstat` is available,
peak RSS for each backend in an isolated process when GNU `/usr/bin/time` is
available, plus Linux `/proc` RSS/high-water samples at indexing and status
boundaries, and a median summary under `target/benchmark-results/`. Stage marks
show when a peak grows; they do not attribute memory to a specific allocation.
Set
`SYNTAXMESH_BENCH_ITERATIONS` (default 3) to choose the sample count.


Set `SYNTAXMESH_CONTEXT_EVAL_SCOPE=repository` together with a sibling evaluation
root to index every supported file selected by the same ignore-aware scanner.
The default remains `selected`. Repository mode records its scope and the full
input fingerprint, but still evaluates the existing fixed-target queries; it
measures their behavior amid a full corpus, not comprehensive semantic relevance.
Fixtures use uniquely owned `tempfile` directories, following Shardline's test
pattern. On Unix, set `TMPDIR` to an existing scratch directory with sufficient
space when the system temporary filesystem is too small for whole-repository
databases. Evidence metadata records `scratch_root`; fixture cleanup removes only
its owned child directory, never the scratch parent. There is no automatic disk
fallback or deletion of other runs.
Benchmark child stdout/stderr are written directly to create-new evidence files
while the process runs, including prebuild output. An interrupted run can leave
partial logs without `samples.json`; partial logs are not a completed result.
This preserves emitted diagnostics across parent-process interruption, not a
power-loss durability guarantee.
Set `SYNTAXMESH_CONTEXT_BENCH_PROFILE=release` for optimized evaluation builds;
the default is `debug`. The evidence metadata records the profile. Do not compare
debug-build database validation costs with deployment latency as if equivalent.


On Linux with `strace` installed, `cargo make benchmark-file-writes` runs the
same benchmark while counting successful write-family syscalls attributed to
each temporary database and its WAL/SHM files. Set `SYNTAXMESH_BENCH_ROOT` to
choose the repository. Tracing materially distorts timing; use this command for
the byte counts only. It excludes mmap dirty-page writeback and kernel/device
caching, so the result is not total physical I/O or a complete write-amplification
measurement.

For cold backup, store checks, migration precautions, and rebuild after local
store corruption, see the [v0 operations runbook](docs/OPERATIONS.md). The
runbook calls out current recovery limits; NDJSON export is not a database
backup or restore format.
