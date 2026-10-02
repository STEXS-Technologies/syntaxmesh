# SyntaxMesh — TODO / Source of Truth v5

> Working name: **SyntaxMesh**
>
> Goal: build a local-first, runtime-agnostic, Rust-native source and system intelligence engine for software repositories and engineering systems, usable as an embeddable library or standalone service for code navigation, architecture analysis, CI/PR intelligence, IDE/tooling backends, dependency analysis, historical analytics, refactoring support, documentation, security analysis, cloud/control-plane tooling, runtime topology analysis, and coding agents.
>
> This is not intended to be a line-for-line Graphify port. The objective is to preserve the useful product ideas—deterministic AST extraction, graph-native navigation, provenance, multi-language analysis, MCP access—and replace the architectural constraints of a Python/NetworkX/`graph.json` system with a typed, transactional, incremental Rust architecture.
>
> Status of this document: architecture and implementation roadmap, expanded with the semantic world-model / ontology / reasoning plane and a provider-driven engineering-evidence architecture.
>
> Research baseline: 2026-09-22.
>
> v2 expansion: 2026-09-23 — adds ontology, knowledge representation, deterministic inference, conceptual identity, contracts/invariants, data-flow/effect semantics, behavioral models, semantic diff, counterfactual analysis, truth maintenance, architectural drift, proof trees, contradiction discovery, and evidence-backed change requirements on top of the existing graph engine.
>
> v3 expansion: 2026-09-23 — adds precision-analysis providers, program-analysis overlays, ontology interfaces/capabilities, named future-state scenarios, ownership/catalog evidence, runtime dependency summaries, incident/problem/decision lineage, agent-context evaluation and historical replay, engineering-policy evaluation, and a stronger external-evidence reconciliation model. It also sharpens the product boundary: SyntaxMesh computes change impact, requirements, constraints, evidence, and verification predicates; a separate upper-layer change engine owns executable planning, transformation, and verification workflows.
>
> v4 expansion: 2026-09-23 — reserves and specifies a first-class coding-harness integration architecture, using the open-source OpenAI Codex Rust harness as the reference implementation and benchmark target. SyntaxMesh itself remains completely harness/runtime agnostic. A separate adapter layer may embed the engine directly, run it as a sidecar, expose it through MCP, or integrate it into host-owned context/world-state lifecycles. v4 defines the public-fork experiment, worktree-aware incremental lifecycle, context-pack injection, native semantic tools, post-edit semantic verification, benchmark methodology, licensing boundaries, failure semantics, and an upstream-friendly integration strategy.
>
> v5 expansion: 2026-09-25 — makes temporal system intelligence and change lineage first-class. SyntaxMesh versions not only code/graph state but also semantic state, declarations, documents, contracts, runtime evidence, and their relationships across generations. It adds logical ChangeSet identity, bitemporal-style fact validity/observation semantics, change-to-change dependency lineage, direct/derived/observed/correlated consequence classes, delayed and cross-layer consequence discovery, multi-generation impact evolution, historical origin/decision tracing, first-possible/first-observed/last-observed queries, long-range propagation analysis, and history-aware agent context. v5 explicitly does NOT introduce a gimmicky 'butterfly effect' primitive: surprising long-range consequences emerge from ordinary versioned evidence and consequence-lineage queries when the data supports them.

---

## 0. Core product thesis

SyntaxMesh should be treated as an **incremental source-intelligence database**, not as a graph-generation CLI.

The core mental model should be:

```text
filesystem / git / build metadata / schemas / docs
                    |
                    v
        incremental ingestion engine
                    |
                    v
     typed source-intelligence facts
                    |
                    v
       transactional live graph store
                    |
          +---------+----------+
          |                    |
          v                    v
  graph execution layer     DuckDB analytics
          |                    |
          +---------+----------+
                    |
                    v
          query/application layer
                    |
      +-------------+-------------+-------------+-------------+
      |             |             |             |             |
      v             v             v             v             v
     CLI         HTTP/API      IDE/LSP         CI/PR          MCP
      |             |             |             |             |
      +-------------+-------------+-------------+-------------+
                    |
                    v
      humans, tools, services, and agents
```

The database is not an export artifact.

The database is the current source of truth.

JSON, GraphML, Cypher, HTML, Markdown, snapshots, and other formats are **exports derived from canonical state**.

---

# 1. Architectural principles

These should be treated as invariants.

- [ ] `syntaxmeshd` MUST be only one host for SyntaxMesh; the core intelligence engine MUST be embeddable directly into other Rust systems.
- [ ] SyntaxMesh core/domain crates MUST NOT assume a daemon, HTTP server, MCP transport, CLI, Tokio runtime, Turso, DuckDB, Penelope, or StateChronicle.
- [ ] Runtime probes/SDK adapters MUST be lightweight and MUST NOT require the full SyntaxMesh engine.
- [ ] The full SyntaxMesh engine MUST support standalone and embedded hosting with equivalent logical behavior.
- [ ] SyntaxMesh MUST be open source.
- [ ] The public project MUST expose a stable extension SDK so users can model proprietary infrastructure without forking SyntaxMesh.
- [ ] Official first-party system/framework integrations in the public SyntaxMesh project MUST be limited to STEXS-Technologies projects that are themselves open source.
- [ ] Closed-source STEXS platform integrations MUST live outside the public SyntaxMesh repository and consume the same public extension interfaces available to external users.
- [ ] Proprietary integration semantics MUST NOT leak into `syntaxmesh-core`.
- [x] The live graph MUST NOT require loading one giant JSON document.
- [ ] The graph MUST be incrementally updateable at file/symbol granularity.
- [x] All updates MUST be atomic and transactional.
- [ ] Every graph fact MUST have provenance.
- [ ] Extracted facts and inferred facts MUST remain distinguishable.
- [ ] Parsing and symbol/reference resolution MUST be separate stages.
- [ ] Language-specific logic MUST stay outside the language-independent graph core.
- [ ] Storage MUST sit behind a stable trait boundary.
- [ ] Turso Database MUST be the primary live-store implementation from the first usable version.
- [ ] SQLite compatibility MUST remain a supported fallback/reference backend behind the same storage abstraction.
- [ ] DuckDB should be first-class from the architecture level, but used for analytics/history rather than live OLTP graph authority.
- [ ] Graph algorithms MUST NOT depend on executing recursive SQL for every traversal.
- [ ] The query engine MUST support token-budgeted context generation for agents.
- [ ] Git branches, commits, worktrees, and working-tree state should be representable without cloning the entire graph.
- [ ] Stable semantic identifiers MUST be separate from physical database row IDs.
- [ ] The code index MUST be disposable and rebuildable from source.
- [ ] No LLM should be required to understand ordinary source-code structure.
- [ ] Optional LLM enrichment MUST never overwrite deterministic facts.
- [ ] Penelope MUST be part of the full SyntaxMesh engine's internal reliability model from the first implementation phase; it is not a user-disableable engine mode, but `syntaxmesh-core`, runtime-protocol crates, and thin SDK/game probes MUST remain Penelope-free.
- [ ] StateChronicle integration MUST exist from the first implementation phase, while verified graph history remains opt-in at runtime initially.
- [ ] Enabling or disabling StateChronicle verified history MUST NOT change graph semantics, stable IDs, query behavior, or canonical Turso state.
- [ ] Graph visualization MUST remain outside SyntaxMesh core; SyntaxMesh provides stable structured NDJSON graph/query exports instead.
- [ ] Same input + same extractor versions + same configuration should produce deterministic structural facts.
- [ ] All schemas and public interchange formats MUST be versioned from day one.
- [ ] Storage migrations MUST be explicit, tested, reversible when practical, and crash-safe.
- [ ] Core functionality should work fully offline.
- [ ] SyntaxMesh MUST NOT depend on Turso Cloud or any hosted database/service.
- [ ] Cloud deployment means running the same self-contained SyntaxMesh engine on cloud infrastructure with embedded/local Turso storage.
- [ ] A live Turso database MUST have one owning SyntaxMesh process/workspace runtime; do not place the live DB on shared network storage for concurrent access.
- [ ] Turso usage MUST be embedded/local-only; no Turso Cloud dependency, hosted coordination, or required remote service.
- [ ] Agent-facing interfaces should return compact evidence, not dump whole graphs.

---

# 2. Database strategy

## 2.1 Primary live database: Turso Database

Turso Database is a day-zero dependency and the primary storage target.

This is an intentional project-level choice, not an optional future backend.

Why:

- Rust-native implementation
- SQLite-compatible file format and SQL frontend
- native async-oriented architecture
- upstream direction includes concurrent writes, CDC, vector search, and improved FTS
- long-term concurrent-write model is a much better architectural fit for independently processed repository partitions
- keeping the source-intelligence engine and embedded database both Rust-native simplifies packaging and integration

However, **do not conflate "use Turso immediately" with "enable experimental local MVCC everywhere immediately."**

As of 2026-09-22, Turso documents local MVCC as experimental and lists limitations that directly matter here, including lack of usable indexes in MVCC mode and potentially incorrect behavior. SyntaxMesh requires indexes for practical graph lookup, so the production local mode must remain correctness-first until upstream removes those blockers.

Therefore support three Turso operating profiles:

```text
1. embedded-wal
   Turso local WAL mode
   stable default initially
   many readers / coordinated writer

2. embedded-mvcc
   Turso local MVCC + BEGIN CONCURRENT
   experimental feature gate
   enabled only when upstream capabilities satisfy SyntaxMesh requirements

3. embedded-only
   No cloud dependency and no hosted control plane.
   SyntaxMesh must run fully locally with an in-process Turso database.
```

The core architecture should be concurrency-ready from the start even when the default local journal mode is WAL.

Recommended ingestion topology initially:

```text
scanner workers
      |
parser workers
      |
resolver workers
      |
 partitioned GraphDelta batches
      |
      v
Turso transaction coordinator
      |
      v
Turso WAL
```

Future MVCC topology:

```text
partition A writer ----\
partition B writer -----+--> Turso BEGIN CONCURRENT / MVCC
partition C writer -----+
partition D writer ----/
```

Transactions should be partitioned so unrelated files/symbols minimize row-level conflicts.

Do not use global AUTOINCREMENT-style hot rows in high-frequency write paths where avoidable. Prefer application-generated stable identifiers and carefully designed physical IDs.

### Turso configuration / capability detection

- [ ] Detect Turso version/capabilities at startup.
- [ ] Detect whether requested journal/concurrency mode is supported.
- [ ] Refuse unsafe experimental modes unless explicitly enabled.
- [ ] Maintain a capability matrix in tests.
- [ ] Keep transaction retry logic at the store layer.
- [ ] Benchmark WAL and concurrent modes separately.
- [ ] Keep DB backups/snapshots independent of engine assumptions.

---

## 2.2 libSQL clarification

The likely SQLite fork being remembered is **libSQL**.

However, libSQL is not the multi-writer answer.

The libSQL project currently explicitly states that it remains a SQLite fork and inherits SQLite's fundamental single-writer limitation.

Therefore:

- [ ] Do not choose libSQL merely for concurrent writers.
- [ ] Consider libSQL only if embedded replication / remote access becomes useful.
- [ ] Keep compatibility possible through the storage abstraction.
- [ ] Do not introduce libSQL as a mandatory dependency.

---

## 2.3 Turso concurrency strategy

The Turso team distinguishes:

- `libSQL`: SQLite fork that still inherits the fundamental single-writer model
- `Turso Database`: from-scratch Rust implementation with a concurrency architecture capable of `BEGIN CONCURRENT`/MVCC

Turso is therefore the correct primary engine for SyntaxMesh.

The design target is:

```text
GraphStore
    |
    +-- graph-store-turso      [primary]
    |
    +-- graph-store-sqlite     [compatibility/reference fallback]
```

Turso-specific APIs must still not leak into graph-core.

### Turso TODO

- [ ] Implement Turso backend in Phase 1.
- [ ] Build the shared storage conformance suite before substantial language work.
- [ ] Verify indexes, FTS requirements, transaction behavior, migrations, and crash recovery.
- [ ] Keep WAL mode as the safe embedded default while local MVCC cannot satisfy required indexed workloads.
- [ ] Implement experimental `BEGIN CONCURRENT` support behind a feature/config gate.
- [ ] Implement bounded retry with jitter for MVCC write/write conflicts.
- [ ] Partition graph writes by repository/file/fact ownership to reduce conflicts.
- [ ] Avoid unnecessary shared hot rows/counters.
- [ ] Benchmark:
  - one writer
  - 2 writers
  - 4 writers
  - 8 writers
  - 16 writers
  - hot-row conflict workload
  - unrelated-file ingestion workload
  - large batch inserts
  - mixed reads/writes
- [ ] Keep a SQLite backend for differential testing and portability.
- [ ] Turso Cloud MUST NOT be required, supported, or assumed by the core deployment model.
- [ ] Promote local MVCC to the default only when upstream supports SyntaxMesh's required indexes/queries reliably.

---

## 2.4 Do not build on SQLite's experimental `BEGIN CONCURRENT` branch

SQLite itself has an experimental `BEGIN CONCURRENT` implementation outside normal trunk releases.

It allows concurrent write transactions but uses optimistic conflict detection and still serializes part of commit behavior.

This should be treated as research/reference material, not as the project's production dependency.

- [ ] Do not ship a custom SQLite fork as the default.
- [ ] Do not depend on WAL2/non-trunk SQLite behavior.
- [ ] Avoid owning database-engine maintenance unless absolutely necessary.

---

## 2.5 FrankenSQLite and other emerging engines

Other Rust SQLite-compatible efforts are appearing, including FrankenSQLite.

Interesting ideas include:

- MVCC
- concurrent writers
- Rust memory safety
- stronger durability designs

But do not make an early-stage database engine the canonical index backend just because its feature list looks attractive.

Keep the backend trait clean enough to experiment later.

---

# 3. DuckDB integration

DuckDB should be integrated intentionally from the beginning, but not as the OLTP graph database.

Use this split:

```text
                 LIVE SYSTEM
              +----------------+
              | SQLite / Turso |
              +----------------+
                      |
           snapshot / Arrow / Parquet
                      |
                      v
              +----------------+
              |     DuckDB     |
              +----------------+
                 ANALYTICS
```

SQLite/Turso responsibilities:

- current repository state
- incremental mutation
- point lookup
- symbol lookup
- edge lookup
- provenance
- file state
- working tree
- MCP interactive queries
- live updates

DuckDB responsibilities:

- historical analysis
- large aggregation
- cross-repository analytics
- architecture trend analysis
- graph metrics over time
- benchmark datasets
- CI history
- ownership statistics
- churn analysis
- dependency growth
- coupling trends
- graph snapshots
- exported Parquet datasets
- large scans and joins

## 3.1 Avoid dual-write transactions

Do not make every graph update synchronously write both SQLite and DuckDB.

That creates distributed-consistency problems inside one local application.

Instead:

```text
commit to canonical store
        |
        +--> append change journal
        |
        +--> async/batched analytics export
```

DuckDB can consume:

- versioned snapshots
- append-only change events
- Parquet partitions
- Arrow record batches

The live graph remains correct even if DuckDB analytics are temporarily stale.

## 3.2 DuckDB datasets

Create analytical tables/views for:

- [ ] repository snapshots
- [ ] file history
- [ ] symbol history
- [ ] edge history
- [ ] graph metrics
- [ ] community membership
- [ ] code churn
- [ ] git authorship
- [ ] ownership
- [ ] test coverage
- [ ] build failures
- [ ] architecture-rule violations
- [ ] agent queries
- [ ] retrieval results
- [ ] query latency
- [ ] token usage
- [ ] index build timings
- [ ] language extractor metrics
- [ ] confidence/provenance distributions

Example questions DuckDB should eventually answer efficiently:

```text
Which modules gained the most dependencies over the last 90 days?

Where is coupling increasing fastest?

Which files repeatedly become high-betweenness nodes?

Which teams own the highest-blast-radius components?

Which architecture boundaries have eroded since release 2.0?

Which symbols frequently participate in regressions?

Which agent query types generate the most unnecessary source reads?

Which language extractors create the most unresolved references?
```

## 3.3 DuckDB transport

Preferred:

```text
canonical DB
   |
   +--> Arrow batches
   +--> Parquet partitions
   +--> change-event log
              |
              v
            DuckDB
```

Do not couple the analytics layer directly to internal SQLite table layout if it can be avoided.

Define a stable analytical interchange schema.

---

# 4. Rust workspace layout

Recommended workspace:

```text
crates/
    syntaxmesh-core/
    syntaxmesh-ids/
    syntaxmesh-model/

    syntaxmesh-store/
    syntaxmesh-store-turso/
    syntaxmesh-store-sqlite/

    syntaxmesh-analytics/
    syntaxmesh-analytics-duckdb/

    syntaxmesh-source/
    syntaxmesh-scanner/
    syntaxmesh-watch/
    syntaxmesh-git/

    syntaxmesh-syntax/
    syntaxmesh-language-sdk/
    syntaxmesh-resolver/

    syntaxmesh-lang-rust/
    syntaxmesh-lang-python/
    syntaxmesh-lang-javascript/
    syntaxmesh-lang-typescript/
    syntaxmesh-lang-go/
    syntaxmesh-lang-java/
    ...

    syntaxmesh-graph/
    syntaxmesh-algorithms/
    syntaxmesh-query/
    syntaxmesh-search/
    syntaxmesh-ranking/
    syntaxmesh-context/

    syntaxmesh-provenance/
    syntaxmesh-snapshot/
    syntaxmesh-diff/
    syntaxmesh-rules/

    syntaxmesh-docs/
    syntaxmesh-schema/
    syntaxmesh-infra/
    syntaxmesh-semantic/

    syntaxmesh-export/
    syntaxmesh-mcp/
    syntaxmesh-http/
    syntaxmesh-daemon/
    syntaxmesh-cli/

    syntaxmesh-bench/
    syntaxmesh-testkit/

xtask/
```

Rules:

- [ ] `syntaxmesh-core` must not depend on SQLite, DuckDB, Tree-sitter, HTTP, MCP, Tokio runtime specifics, or CLI libraries.
- [ ] Storage crates depend inward toward core/model.
- [ ] Language crates implement language SDK traits.
- [ ] MCP/HTTP/CLI are adapters around application services.
- [ ] Analytics is downstream from canonical facts.
- [ ] Avoid one giant `syntaxmesh` crate becoming a dependency sink.

---

# 5. Core typed model

Avoid Python-style generic dictionaries as subsystem contracts.

Example:

```rust
pub struct Node {
    pub id: NodeId,
    pub stable_id: StableNodeId,
    pub kind: NodeKind,
    pub name: SmolStr,
    pub qualified_name: Option<SmolStr>,
    pub owner: Option<FileId>,
    pub span: Option<SourceSpan>,
    pub language: Option<LanguageId>,
    pub attributes: NodeAttributes,
}

pub struct Edge {
    pub id: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
    pub relation: RelationKind,
    pub certainty: Certainty,
    pub provenance: ProvenanceId,
}

pub enum Certainty {
    SourceFact,
    StaticallyResolved,
    Heuristic,
    SemanticInference,
    UserAsserted,
}
```

Do not flatten everything into string-keyed metadata.

Provide extension fields where necessary, but keep common graph semantics typed.

---

# 6. Stable identity

Physical DB IDs are implementation details.

Every semantic entity needs a stable identity.

Potential inputs:

```text
repository identity
normalized path
language
symbol kind
qualified name
lexical scope
semantic disambiguator
```

Use BLAKE3 for stable content/identity hashes unless a stronger compatibility requirement appears.

Example:

```text
stable_symbol_id =
BLAKE3(
    repository_namespace ||
    normalized_relative_path ||
    language ||
    symbol_kind ||
    qualified_name ||
    disambiguator
)
```

Requirements:

- [ ] IDs survive database compaction.
- [ ] IDs survive re-indexing.
- [ ] IDs survive process restarts.
- [ ] IDs should survive line-number changes.
- [ ] Renames should be detectable separately from identity where possible.
- [ ] Symbol moves should support similarity-based continuity.
- [ ] Do not make line number part of primary semantic identity.
- [ ] Maintain a rename/move history when confidence is high.

---

# 7. Canonical schema

At minimum:

```text
repositories
snapshots
working_trees
files
file_versions
nodes
node_versions
edges
edge_versions
references
provenance
evidence
extractor_runs
extractor_versions
communities
architecture_rules
rule_violations
queries
query_evidence
```

Possible SQLite schema concepts:

```sql
CREATE TABLE repositories (
    id              INTEGER PRIMARY KEY,
    stable_id       BLOB NOT NULL UNIQUE,
    root            TEXT NOT NULL,
    vcs_kind        INTEGER NOT NULL
);

CREATE TABLE files (
    id              INTEGER PRIMARY KEY,
    repository_id   INTEGER NOT NULL,
    path            TEXT NOT NULL,
    language        INTEGER,
    UNIQUE(repository_id, path)
);

CREATE TABLE file_versions (
    id              INTEGER PRIMARY KEY,
    file_id         INTEGER NOT NULL,
    content_hash    BLOB NOT NULL,
    size_bytes      INTEGER NOT NULL,
    mtime_ns        INTEGER,
    indexed_at      INTEGER NOT NULL,
    extractor_set   BLOB NOT NULL
);

CREATE TABLE nodes (
    id              INTEGER PRIMARY KEY,
    stable_id       BLOB NOT NULL UNIQUE,
    kind            INTEGER NOT NULL,
    name            TEXT NOT NULL,
    qualified_name  TEXT
);

CREATE TABLE node_locations (
    node_id          INTEGER NOT NULL,
    file_version_id  INTEGER NOT NULL,
    start_byte       INTEGER,
    end_byte         INTEGER,
    start_line       INTEGER,
    end_line         INTEGER,
    PRIMARY KEY(node_id, file_version_id)
);

CREATE TABLE edges (
    id              INTEGER PRIMARY KEY,
    stable_id       BLOB NOT NULL UNIQUE,
    source_id       INTEGER NOT NULL,
    target_id       INTEGER NOT NULL,
    relation        INTEGER NOT NULL,
    certainty       INTEGER NOT NULL,
    provenance_id   INTEGER NOT NULL
);

CREATE INDEX edge_source_relation
ON edges(source_id, relation);

CREATE INDEX edge_target_relation
ON edges(target_id, relation);

CREATE INDEX node_name
ON nodes(name);
```

Actual schema should be normalized based on benchmarked query paths, not ideology.

---

# 8. First-class provenance

One of the biggest opportunities is to make evidence stronger than a string field attached to an edge.

Every fact should answer:

```text
Why does SyntaxMesh believe this?
```

Example:

```text
Edge:
  AuthController --CALLS--> AuthService.verify

Provenance:
  kind: STATICALLY_RESOLVED
  extractor: typescript-v3
  resolver: ts-module-resolver-v2
  source_file: src/auth/controller.ts
  source_hash: ...
  source_span: bytes 1450..1477
  lines: 52..52
  observed_text_hash: ...
  created_in_index_run: ...
```

For inferred facts:

```text
kind: SEMANTIC_INFERENCE
model_provider: ...
model_name: ...
model_revision: ...
prompt_fingerprint: ...
temperature: ...
input_hashes: [...]
confidence: ...
```

This directly avoids provenance ambiguity and model-blind caching problems.

Requirements:

- [ ] Record extractor version.
- [ ] Record resolver version.
- [ ] Record language grammar version.
- [ ] Record model/provider for semantic extraction.
- [ ] Record prompt/template fingerprint.
- [ ] Include configuration fingerprint in semantic cache keys.
- [ ] Make cache entries invalid when relevant extractor/model/config changes.
- [ ] Allow `syntaxmesh explain <fact-id>`.
- [ ] Allow MCP `explain_fact`.
- [ ] Preserve evidence spans.
- [ ] Never silently promote inferred facts to deterministic facts.

---

# 9. Parsing architecture

Tree-sitter remains useful.

Rust does not magically make Tree-sitter itself much faster, because Tree-sitter is already native code.

The actual performance wins come from:

- less Python object churn
- less NetworkX overhead
- typed compact structures
- direct memory ownership
- reduced serialization
- no giant JSON round trips
- parallel orchestration
- incremental invalidation
- compact graph projections
- batched persistence

## 9.1 Language extractor API

Example:

```rust
pub trait LanguageExtractor: Send + Sync {
    fn language(&self) -> LanguageId;

    fn extract(
        &self,
        source: &SourceFile,
        sink: &mut dyn ExtractionSink,
    ) -> Result<(), ExtractError>;
}
```

Typed output:

```rust
pub enum ExtractionEvent {
    Definition(Definition),
    Reference(Reference),
    Import(ImportRecord),
    Export(ExportRecord),
    Annotation(Annotation),
    Diagnostic(ExtractionDiagnostic),
}
```

Do not force the parser to resolve references immediately.

---

# 10. Separate reference extraction from resolution

For:

```text
foo.bar()
```

the parser may know:

```text
call expression
receiver = foo
member = bar
scope = current function
```

It may not yet know the exact target.

Emit a reference.

Then resolve in a separate phase using:

- local lexical scope
- imports
- aliases
- namespace/package rules
- type information
- inheritance
- traits/interfaces
- re-exports
- module systems
- workspace/package metadata
- compiler metadata where available

This separation is essential for maintainability across many languages.

---

# 11. Add precision without launching analyzed-language runtimes

Language extraction and resolution run in Rust and remain limited to the
approved source formats in [§41](#41-language-support-order). Do not integrate
compiler APIs, LSP servers, interpreters, package managers, or tooling that
requires launching a TypeScript/JavaScript, Python, Bash, or other non-Rust
runtime. Such tooling would violate SyntaxMesh's Rust-only execution boundary.

Precision may instead improve through Rust-native parsers/resolvers and
static inspection of repository metadata already in the source tree (for
example `Cargo.toml`, `tsconfig.json`, `package.json`, and `pyproject.toml`).
Metadata interpretation must remain conservative, versioned, and provenance-
backed; do not imply that reading a config file reproduces compiler/runtime
semantics. Facts must retain provenance so the user knows which source
produced them.

---

# 12. Incremental engine

This should be one of the main differentiators.

## 12.1 File detection

For each file track:

- path
- size
- mtime
- BLAKE3 content hash
- language
- parser version
- extractor configuration fingerprint

Fast path:

```text
mtime + size unchanged
        |
        v
probably unchanged
```

Correctness path:

```text
content hash
```

Do not trust mtime alone.

## 12.2 File-level transaction

Simplest safe initial implementation:

```text
BEGIN

identify old facts owned by file version
insert new file version
insert extracted definitions
insert unresolved references
resolve affected references
replace affected graph facts
update current-file pointer

COMMIT
```

Later optimize to true semantic diff instead of delete/reinsert.

## 12.3 Dependency-aware invalidation

If `foo` changes, do not blindly reparse the repository.

Track dependency classes:

```text
file changed
    |
    +-- local structural changes only
    |
    +-- exported symbol changes
    |
    +-- type/signature changes
    |
    +-- module resolution changes
```

Only re-resolve dependents when exported semantic surface changes.

## 12.4 Crash consistency

- [ ] Index update must be atomic.
- [ ] A crash must leave the previous graph valid.
- [ ] Resume metadata should be transactional.
- [ ] Partial semantic/LLM jobs should be checkpointable.
- [ ] Checkpoints should live in the database, not scattered files.
- [ ] Never produce a half-written canonical graph.

This structurally avoids races associated with multiple processes rewriting the same `graph.json`.

---

# 13. Watch mode and daemon

Provide a long-running daemon:

```text
syntaxmeshd
```

Responsibilities:

- filesystem watch
- debounce/coalescing
- git-state monitoring
- background incremental indexing
- MCP serving
- graph projection maintenance
- analytics export scheduling
- health/status
- local IPC

A single daemon can coordinate writes and prevent multiple concurrent CLI invocations from corrupting state.

CLI commands can connect to daemon when available.

Fallback to direct embedded operation when daemon is absent.

---

# 14. In-memory graph execution layer

Do not use SQLite as a BFS engine.

SQLite should answer:

```text
find node
find metadata
find evidence
find initial adjacency range
```

Graph algorithms should use a compact Rust projection.

Initial representation:

```rust
struct GraphIndex {
    outgoing: Vec<Vec<EdgeRef>>,
    incoming: Vec<Vec<EdgeRef>>,
}
```

Scale representation:

- CSR
- CSC for reverse traversal
- compact numeric node IDs
- packed edge relation codes
- optional memory-mapped projections

Algorithms:

- [ ] BFS
- [ ] DFS
- [ ] shortest path
- [ ] multi-source shortest path
- [ ] reverse dependency traversal
- [ ] strongly connected components
- [ ] cycle detection
- [ ] dominators where useful
- [ ] PageRank
- [ ] betweenness approximations
- [ ] connected components
- [ ] blast-radius calculation
- [ ] dependency layers
- [ ] reachability
- [ ] community detection
- [ ] architecture boundary analysis

Maintain projections incrementally when practical.

---

# 15. Query engine

Do not expose the raw storage model directly to agents.

Build an intermediate graph query representation.

Example:

```rust
pub enum QueryOp {
    SearchSymbols { query: String },
    Expand {
        seeds: Vec<NodeId>,
        direction: Direction,
        relations: RelationFilter,
        depth: u8,
    },
    Path {
        from: NodeSelector,
        to: NodeSelector,
        constraints: PathConstraints,
    },
    Impact {
        target: NodeSelector,
        max_depth: u8,
    },
    Evidence {
        facts: Vec<FactId>,
    },
}
```

Later add a textual query DSL if demand exists.

Do not invent a large Cypher clone before the basic agent use cases are measured.

---

# 16. Search and retrieval

Use multiple signals.

Deterministic baseline:

- exact symbol match
- qualified symbol match
- path match
- SQLite FTS5
- edge proximity
- graph centrality
- relation relevance

Optional semantic layer:

- embeddings
- local model
- externally configured model

Do not make vectors mandatory.

Hybrid retrieval can rank:

```text
lexical score
+ graph proximity
+ symbol/type match
+ provenance quality
+ optional semantic similarity
+ recency/change relevance
```

---

# 17. Context compiler for coding agents

This should be a major feature rather than a side effect.

Agent asks:

```text
How does authentication reach the database?
```

Pipeline:

```text
natural-language query
        |
        v
intent/query planning
        |
        v
symbol candidates
        |
        v
graph traversal
        |
        v
path ranking
        |
        v
source/evidence fetch
        |
        v
token-budget optimizer
        |
        v
compact context pack
```

Context pack should include:

- relevant symbols
- source snippets
- exact file/line evidence
- relationship explanation
- architecture context
- ambiguity warnings
- omitted-context summary
- token count

## 17.1 Exact token budgets

Support:

```text
--max-tokens 2000
--max-tokens 8000
```

Degradation strategy:

1. exact evidence snippets
2. signatures
3. graph paths
4. selected summaries
5. lower-ranked context removed first

Do not simply truncate output bytes.

---

# 18. MCP surface

Initial MCP tools:

```text
status
search
node
neighbors
path
impact
explain
context
diff
architecture_violations
query_history
```

Avoid exposing generic SQL write access.

Agent should interact through semantic operations.

Every response should support:

- result count
- evidence
- truncation indication
- graph version
- working-tree/commit identity
- query latency

Because the canonical database is live, MCP should never need to reload a monolithic `graph.json` after every update.

---

# 19. Git-native model

Represent Git explicitly.

Entities:

```text
Repository
Commit
BranchRef
Worktree
WorkingTreeSnapshot
FileVersion
GraphSnapshot
```

Do not create one full database per branch unless necessary.

Aim for structural sharing.

Potential model:

```text
base commit snapshot
       |
       +--> changed file versions
       +--> changed facts
       +--> overlay graph
```

Support:

- [ ] graph at HEAD
- [ ] graph at working tree
- [ ] graph at arbitrary commit
- [ ] graph diff between commits
- [ ] branch comparison
- [ ] PR graph
- [ ] worktree isolation
- [ ] release comparison

---

# 20. Historical graph

Once snapshots exist, build temporal intelligence.

Queries:

```text
When did this dependency appear?

Which release introduced this cycle?

How has AuthService blast radius changed?

Which modules are accumulating coupling?

Which symbols repeatedly move across boundaries?

Who usually changes this subsystem?

What architectural relationship existed when bug X was introduced?
```

Store high-volume history in DuckDB/Parquet where appropriate.

---

# 21. Architecture rule engine

Add architecture constraints.

Example config:

```toml
[[rule]]
name = "domain-must-not-depend-on-http"
from = "crate:domain"
to = "crate:http"
relations = ["imports", "calls"]
action = "deny"
```

More examples:

- service may not depend on adapter
- domain may not import infrastructure
- package A may call package B only through interface C
- public API cannot depend on experimental module
- no cycle across workspace package boundaries
- database access must go through repository layer

Expose in:

- CLI
- CI
- MCP
- PR checks

---

# 22. Graph diffs

Graph diff should be first-class.

Not merely:

```text
+ file
- file
```

But:

```text
+ symbol
- symbol
renamed symbol
moved symbol
+ dependency
- dependency
changed public signature
increased blast radius
new cycle
removed cycle
boundary violation introduced
centrality changed materially
```

This is highly valuable for code review.

---

# 23. PR intelligence

Later:

```text
syntaxmesh pr analyze
```

Should report:

- changed symbols
- affected callers
- affected tests
- dependency changes
- architecture violations
- newly unreachable/dead symbols
- cycle changes
- likely review hotspots
- ownership
- provenance-backed evidence

Do not let an LLM invent the blast radius.

Use deterministic graph facts and optionally let an LLM explain them.

---

# 24. Test/build integration

Add non-source facts:

```text
test --COVERS--> symbol
build_target --CONTAINS--> module
ci_job --RUNS--> test_suite
fixture --USED_BY--> test
```

Sources:

- cargo metadata
- test runner output
- coverage formats
- CI metadata
- build systems
- Bazel
- Cargo
- npm/pnpm
- Gradle/Maven
- Go tooling

Then agent queries improve:

```text
Which tests cover this symbol?

What must run if this changes?

Why does this package rebuild?

What downstream targets are affected?
```

---

# 25. Schema and infrastructure intelligence

Graphify's "everything in one graph" direction is useful and should be retained, but with typed domains.

Support:

SQL:

```text
table
column
foreign key
view
index
migration
```

Infrastructure:

```text
container
service
queue
topic
bucket
database
deployment
environment variable
secret reference
```

API:

```text
route
handler
request model
response model
client call
OpenAPI operation
GraphQL field
```

Config:

```text
config key
producer
consumer
default
override
```

Example cross-domain path:

```text
POST /users
 -> create_user_handler
 -> UserService.create
 -> UserRepository.insert
 -> users table
```

That is much more useful than a code-only call graph.

---

# 26. Runtime trace enrichment

Optional future feature:

Ingest runtime traces to distinguish:

```text
STATICALLY_POSSIBLE
```

from:

```text
OBSERVED_AT_RUNTIME
```

Potential integrations:

- OpenTelemetry
- test traces
- profiling
- coverage
- eBPF-derived call observations where appropriate

Never replace static relationships with runtime observations; store both.

---

# 27. Documentation / rationale / ADR graph

Retain and expand Graphify's useful rationale concept.

Promote:

- ADRs
- RFCs
- design docs
- `WHY` comments
- `NOTE` comments
- TODOs
- issue references
- commit messages where useful

Relations:

```text
ADR --DECIDES--> component
ADR --SUPERSEDED_BY--> ADR
RFC --PROPOSES--> API
comment --EXPLAINS--> symbol
issue --MOTIVATES--> change
commit --CHANGES--> symbol
```

This gives agents the missing "why", not only the "what".

Current implementation boundary:

- **Implemented baseline:** Markdown documents, heading sections, and exact
  source-backed prose/code/table chunks are deterministic graph facts. Local
  document/source links are retained as references and resolved only when the
  target is indexed. ADR `Decision` and RFC `Proposal` sections also emit
  source-located `decides`/`proposes` relationships for their explicit links to
  exact indexed source files ([ADR-0112](adr/0112-source-grounded-rationale-links.md)).
  An explicit linked `Status: superseded by` ADR target becomes a
  `superseded_by` relation from the old ADR to its successor
  ([ADR-0113](adr/0113-link-explicitly-superseded-adrs.md)).
  Together these relationships cover only authored headings, status metadata,
  and destinations; they do not extract concepts from surrounding prose.
  This makes authored content searchable and its linked rationale navigable;
  it does not infer relationships from free-form prose or map file links to
  specific API members. Unresolved targets remain explicit.
- **Remaining deterministic work:** add source-grounded relations for the other
  promoted artifacts (`WHY`/`NOTE` comments, TODOs, issue references, and useful
  commit messages), plus exact target/member mapping only where source syntax
  gives unambiguous evidence. Do not infer a decision target from a coincidental
  name match or summarize prose as an asserted fact.
- **Optional semantic increment (partially implemented):** the separate
  `syntaxmesh-semantic` boundary now validates provider-derived concepts and
  relationships against exact quoted document chunks; the explicit Engine API
  builds per-file requests or deterministic chunk/byte-bounded batches across
  a generation from indexed chunks plus authored outer-to-inner section
  headings ([ADR-0117](adr/0117-document-section-context-for-semantic-input.md),
  [ADR-0118](adr/0118-bounded-generation-semantic-batches.md)); quotes remain
  exact to chunk text. The Penelope integration journals/recover jobs and
  caches structured output by provider/model/prompt/configuration plus
  text-and-heading content identity ([ADRs 0115](adr/0115-source-grounded-document-semantic-facts.md), [0116](adr/0116-penelope-semantic-enrichment-cache.md), and [0117](adr/0117-document-section-context-for-semantic-input.md)). Facts retain
  `SemanticInference` provenance and are published only through ordinary
  extension ingestion. The Rust CLI now provides opt-in, one-command local
  Ollama-compatible or explicitly permitted remote HTTPS inference, bounded
  parallel requests, per-document cache reuse, and atomic semantic publication.
  Model digests or explicitly asserted revisions participate in cache identity.
  Per-invocation provider-reported token usage is exposed separately from
  metadata requests ([ADR-0130](adr/0130-host-semantic-token-usage.md)).
  Real-model quality evaluation, cross-document evidence synthesis, dedicated
  semantic query surfaces, durable
  usage history, and monetary cost accounting remain open. Semantic work is opt-in
  and failures do not alter deterministic source indexing.

Before adding public fact kinds, relation kinds, or query contracts, record an
ADR defining identity, provenance, source-span/evidence requirements,
resolution behavior, and extractor invalidation. Reuse existing document,
reference, provenance, generation-history, and query paths where they satisfy
those requirements; do not create a parallel document graph or storage format.

---

# 28. Optional semantic/LLM subsystem

Keep it separate.

```text
syntaxmesh-semantic
```

Rules:

- [x] Core code graph needs no LLM.
- [x] LLM facts have separate provenance class.
- [x] Cache key includes provider/model/revision/prompt/config/input hashes.
- [x] User can disable provider network access with `--semantic-offline`: cache-only execution requires an asserted model revision and fails on a cache miss without inference or metadata calls ([ADR-0131](adr/0131-offline-semantic-cache-execution.md)).
- [x] Local models supported through the Rust CLI's Ollama-compatible adapter; mock integration coverage is not real-model quality evidence.
- [x] Semantic failures never invalidate deterministic index.
- [x] Semantic jobs can resume through Penelope and the store's durable-record port.
- [ ] Costs recorded where API usage exists. Per-run reported token totals and missing/invalid report counts are available; durable usage history and monetary pricing remain open.
- [x] Same semantic result can be reused across snapshots in the same durable store when source content and semantic configuration are identical.

---

# 29. Content-addressed ingestion

Use BLAKE3 content hashes as a primitive throughout.

Potentially address:

```text
source blob
parsed representation
extraction result
semantic enrichment
export snapshot
```

by content hash.

Benefits:

- dedupe across branches
- dedupe across worktrees
- dedupe across snapshots
- reuse extraction across repositories containing identical vendored files
- avoid repeated LLM calls
- deterministic cache invalidation

Do not build a full distributed CAS into v0.1 unless measurements justify it.

Start with local content identity and an abstraction.

---

# 30. Snapshot/export format

Do not commit the operational SQLite DB to Git.

Use:

```text
.syntaxmesh/
    index.db          # ignored
    analytics.duckdb  # ignored or rebuildable
```

For sharing:

```text
syntaxmesh snapshot export
```

Produce a deterministic format.

Possible:

```text
snapshot.smx.zst
```

Logical records:

```text
Header
RepositoryRecord
FileRecord
NodeRecord
EdgeRecord
ProvenanceRecord
CommunityRecord
...
```

Requirements:

- sorted deterministic ordering
- schema version
- extractor version metadata
- checksums
- compression
- streaming import
- forward-compatibility strategy
- no dependency on database physical layout

Optional JSON/GraphML/Cypher exports remain available.

---

# 31. Avoid GitHub blob-size issues by design

A monolithic JSON graph can exceed repository-host file-size limits.

SyntaxMesh should not require graph state to be committed as one raw JSON blob.

Options:

- compressed deterministic snapshot
- chunked snapshot
- artifact storage
- rebuild from sources
- CI cache
- content-addressed segments

The default should be a local disposable DB plus optional portable snapshot.

---

# 32. Concurrency architecture

Phase 1:

```text
parallel readers/parsers
        |
        v
GraphDelta batches
        |
        v
single canonical DB writer
```

This is enough for a lot of throughput.

Phase 2 with Turso experimentation:

```text
worker A transaction
worker B transaction
worker C transaction
worker D transaction
        |
        v
row-level MVCC conflict detection
```

But only adopt if benchmarks demonstrate meaningful gains.

Potential bottlenecks may be elsewhere:

- Tree-sitter parse
- filesystem reads
- symbol resolution
- FTS maintenance
- index maintenance
- graph projection rebuild
- semantic extraction

Do not optimize writer concurrency without profiling.

---

# 33. Backpressure

Large repositories need bounded pipelines.

Use bounded channels.

Example:

```text
scan
 |
 v
parse queue [bounded]
 |
 v
resolution queue [bounded]
 |
 v
commit queue [bounded]
```

Avoid reading millions of files/facts into memory before commit.

Track queue saturation metrics.

---

# 34. Memory model

Avoid NetworkX-style heavyweight object-per-node/object-per-edge representation.

Prefer:

- compact integer IDs
- interned strings
- enum relation codes
- arenas/slabs
- packed adjacency
- string tables
- immutable graph projections
- copy-on-write/epoch swap for readers

Potential read model:

```text
current GraphSnapshot pointer
          |
writer builds delta
          |
new snapshot/index generation
          |
atomic generation swap
```

MCP readers can continue serving the old snapshot while a new generation is prepared.

---

# 35. Query consistency

Every query response should state the graph generation/snapshot used.

Example:

```json
{
  "repository": "...",
  "commit": "...",
  "working_tree_generation": 193,
  "index_generation": 5821
}
```

This makes stale-result debugging much easier.

---

# 36. Schema migrations

Implement from day one.

- migration table
- migration checksums
- transactional migration
- backup before destructive migration where necessary
- integration tests from older schema fixtures
- explicit minimum supported schema
- snapshot import migration strategy

Do not rely on users deleting their database after every version bump once adoption begins.

---

# 37. Observability

Local-first does not mean invisible.

Provide:

```text
syntaxmesh status
syntaxmesh doctor
syntaxmesh metrics
```

Metrics:

- files scanned
- files parsed
- files skipped
- cache hit ratio
- AST time
- resolver time
- DB commit time
- DuckDB export time
- projection-build time
- unresolved reference count
- graph nodes/edges
- memory usage
- DB size
- query p50/p95/p99
- MCP request counts
- token savings estimate where measurable

Telemetry should remain opt-in if any remote telemetry is ever introduced.

Local metrics should work without telemetry.

---

# 38. Security

Source intelligence tools handle proprietary code.

Requirements:

- local-only by default
- no hidden network calls
- LLM network use explicitly configured
- clear data-flow documentation
- no source content in error telemetry
- safe path normalization
- symlink handling
- archive bomb limits for imported docs
- parser resource limits
- database corruption handling
- malicious-repository test corpus
- MCP input validation
- query resource limits
- max traversal depth
- max result cardinality
- max source bytes returned
- redaction hooks for secrets

Never automatically send repository data to a cloud model merely because an API key exists.

---

# 39. Secret handling

During indexing:

- detect probable secrets
- do not copy secret values into semantic enrichment
- optionally mark secret-bearing source spans
- allow exclusion policies

The graph may know:

```text
config key DATABASE_URL is consumed by module X
```

without storing the actual credential value.

---

# 40. Plugin/language SDK

Adding a language should not require editing core modules.

Language pack provides:

```rust
pub trait LanguagePlugin {
    fn metadata(&self) -> LanguageMetadata;
    fn grammar(&self) -> GrammarProvider;
    fn extractor(&self) -> &dyn LanguageExtractor;
    fn resolver(&self) -> Option<&dyn LanguageResolver>;
}
```

Each language package includes:

- grammar binding
- file extensions
- ignore rules
- symbol extraction
- import extraction
- reference extraction
- relation mapping
- tests
- corpus fixtures
- benchmark fixtures

---

# 41. Language support order

The supported source-language scope is intentionally bounded to Rust,
TypeScript/JavaScript, Python, and Bash scripts, plus Markdown/plain-text
documentation. Every extractor, parser, resolver, and indexing/query path is
implemented and run by Rust code; SyntaxMesh does not launch the indexed
languages' runtimes. The Node and Python resolution profiles are static
analysis strategies, not Node.js or Python runtime dependencies.

Do not add Go, Java, or another source language/runtime by implication. Any
expansion requires an explicit product-scope decision. Within the supported
set, prioritize correctness and representative corpus coverage over adding
more language names. See [ADR-0078](adr/0078-rust-only-execution-and-input-language-scope.md).

---

# 42. Visualization boundary and structured graph export

SyntaxMesh MUST NOT ship or own a graph visualization frontend as part of the core project.

Visualization is a separate concern with different scaling, layout, interaction, and product requirements. Keeping it outside SyntaxMesh prevents the graph engine from accumulating browser/UI dependencies and lets users choose the visualization stack that fits their use case.

SyntaxMesh's responsibility is to expose a stable, documented, machine-readable graph contract.

## 42.1 NDJSON is the only first-class graph export format

Do not support a monolithic NDJSON graph export.

Use NDJSON / JSON Lines only.

Reason:

- streams naturally
- bounded memory usage
- works for very large graphs
- easy to pipe through Unix tools
- consumers can process records incrementally
- avoids building one huge JSON document
- avoids ambiguous "whole graph object" semantics
- aligns with SyntaxMesh's incremental architecture

Provide:

```text
syntaxmesh export ndjson
```

with selection/filtering:

```text
syntaxmesh export ndjson --all
syntaxmesh export ndjson --node AuthService --depth 2
syntaxmesh export ndjson --path HttpHandler UsersTable
syntaxmesh export ndjson --changed-since main
syntaxmesh export ndjson --relations CALLS,IMPORTS
syntaxmesh export ndjson --layer syntax
syntaxmesh export ndjson --layer architecture
```

The export must support stdout and file output.

Example:

```json
{"type":"header","schema_version":1,"repository":{"id":"repo_..."},"generation":{"id":"gen_...","git_commit":"..."}}
{"type":"node","node":{"id":"node_...","kind":"function","name":"verify_token","qualified_name":"AuthService::verify_token","language":"rust","file":"src/auth/service.rs","span":{"start_line":42,"end_line":66},"attributes":{}}}
{"type":"edge","edge":{"id":"edge_...","source":"node_...","target":"node_...","relation":"CALLS","certainty":"STATICALLY_RESOLVED","provenance_id":"prov_..."}}
{"type":"provenance","provenance":{"id":"prov_...","kind":"STATICALLY_RESOLVED","file":"src/auth/service.rs","start_line":51,"end_line":51}}
{"type":"footer","nodes":1842,"edges":7311,"provenance_records":4207}
```

This is an interchange/export representation, not the canonical operational database.

## 42.2 Record model

Define explicit record kinds.

Initial record set:

```text
header
node
edge
provenance
community
architecture_violation
footer
```

Future record types can be added compatibly.

Each line MUST be independently parseable JSON.

Do not require consumers to buffer previous records except where they intentionally want graph assembly.

## 42.3 Subgraph export must be first-class

For visualization and tooling, exporting the entire repository graph is often the wrong operation.

Support deterministic subgraph extraction based on:

- seed nodes
- depth
- direction
- relation kinds
- graph layers
- file/module/package boundaries
- path queries
- impact queries
- architecture components
- changed graph region
- snapshot/commit
- maximum node/edge count

Example:

```text
syntaxmesh export ndjson \
  --node UserService \
  --depth 3 \
  --relations CALLS,IMPORTS,QUERIES
```

A visualization application can request only what it can sensibly render.

## 42.4 Stable export schema

Publish and version a JSON Schema for each NDJSON record type.

Requirements:

- [ ] stable semantic IDs
- [ ] explicit schema version in header
- [ ] graph generation metadata
- [ ] typed node kinds
- [ ] typed relation kinds
- [ ] optional provenance records
- [ ] optional source spans
- [ ] graph-layer metadata
- [ ] deterministic ordering when requested
- [ ] documented nullability/optional fields
- [ ] forward-compatible extension fields
- [ ] explicit footer counts/checksums when requested

Do not expose internal Turso row IDs as the public graph identity.

## 42.5 Deterministic ordering

Support:

```text
syntaxmesh export ndjson --deterministic
```

Deterministic mode should guarantee a stable ordering for identical logical graph state.

Suggested ordering:

```text
header
nodes sorted by stable ID
edges sorted by stable ID
provenance sorted by stable ID
communities / violations sorted by stable ID
footer
```

This is useful for:

- reproducible artifacts
- diffing
- caching
- signatures
- StateChronicle verification
- test fixtures

The default streaming mode may use a faster natural order if that materially improves export speed.

## 42.6 Checksums and integrity

Optionally emit an integrity footer:

```json
{"type":"footer","records":113842,"nodes":20117,"edges":88104,"blake3":"..."}
```

The checksum should cover the canonical exported record stream when deterministic mode is enabled.

This gives external consumers a cheap way to verify a complete export.

## 42.7 Query API and public graph DTOs

CLI NDJSON export, HTTP streaming responses, MCP graph results, and future IDE integrations should reuse the same public graph DTO model wherever practical.

Avoid separate incompatible representations for every adapter.

Suggested crates:

```text
syntaxmesh-api-model
syntaxmesh-export
```

The transport may differ while the logical node/edge/provenance types remain stable.

## 42.8 Visualization metadata

SyntaxMesh may expose semantic hints useful to external visualization tools:

```text
node kind
graph layer
module/package
language
relation kind
certainty
centrality metrics
community/group membership
source location
change status
```

SyntaxMesh should NOT own:

- graph layout coordinates
- force-directed layout
- WebGL/WebGPU rendering
- canvas/SVG rendering
- browser UI
- zoom/pan interaction
- visual themes
- graph editor state

Those belong to visualization consumers.

If a future separate STEXS project provides visualization, it should consume SyntaxMesh's public query/export API rather than link into private internals.

## 42.9 Interoperability adapters

NDJSON is the required first-class export format.

Later, lightweight adapters MAY translate NDJSON into ecosystem-specific formats:

```text
GraphML
GEXF
DOT
Cypher import scripts
Parquet/Arrow
```

These should preferably live outside the core project or behind separate adapter crates.

Do not add monolithic JSON as a compatibility format unless a concrete external integration requires it.

## 42.10 Visualization as an external ecosystem

The intended model is:

```text
                    SyntaxMesh
                        |
                   NDJSON/query API
                        |
        +---------------+---------------+
        |               |               |
        v               v               v
   custom web UI    Gephi adapter    IDE extension
        |
        v
 D3 / Sigma.js /
 Cytoscape / etc.
```

SyntaxMesh remains headless and tooling-neutral.

---

# 42.4 Day-one STEXS reliability integrations

## Penelope — mandatory internal workflow runtime

Penelope is a **day-one requirement**.

It is not an optional feature flag and there should not be a normal `disable_penelope = true` mode.

Reason:

SyntaxMesh needs exactly one durable workflow model for operations that span multiple steps, persistence boundaries, retries, restarts, or derived subsystems. Supporting a Penelope path and a separate ad-hoc path would create two correctness models and double the failure surface.

Penelope should orchestrate durable workflows such as:

```text
full repository indexing
incremental index generations
large resolver rebuilds
schema/store migrations
snapshot import/export
DuckDB synchronization
graph-generation publication
StateChronicle publication
semantic/LLM enrichment
long-running CI/PR analysis
recovery/reconciliation after interruption
```

Penelope MUST NOT sit in hot read/query paths.

Do not route these through Penelope:

```text
symbol lookup
FTS search
neighbors
BFS/DFS
path queries
impact traversal
single AST parsing function
ordinary Turso reads
```

The boundary is:

```text
                  SyntaxMesh
                      |
           +----------+-----------+
           |                      |
           v                      v
      hot operations       durable workflows
           |                      |
     Turso / graph              Penelope
        engine                  REQUIRED
```

### Penelope generation discipline

Every durable indexing workflow should carry strongly typed correlation identity such as:

```text
RepositoryId
WorktreeId
GenerationId
IndexRunId
FileVersionId
```

Late results from an obsolete generation MUST be rejected rather than committed over newer state.

Example:

```text
generation 104 parses auth.rs
generation 105 starts after auth.rs changes
generation 104 finishes late
```

Generation 104 must be unable to overwrite generation 105.

Use Penelope semantics for:

- idempotency
- durable retries
- stale-result rejection
- compensation where applicable
- reconciliation of ambiguous side effects
- resumability after daemon/process restart
- step-level observability

Penelope should be treated as part of SyntaxMesh's implementation architecture, not presented as a product feature the user needs to understand.

---

## StateChronicle — day-one integration, verified history opt-in

StateChronicle is also a **day-one architectural requirement**, but verified-history recording is initially **opt-in at runtime**.

This means:

```text
implementation support: REQUIRED from first release
runtime verification:   OFF by default initially
```

A user who only wants:

```text
index
search
navigate
analyze
run architecture rules
query graph
```

must not be forced to manage signing identities or retain a cryptographic graph history.

Enable it explicitly:

```toml
[history]
verified = true
```

or:

```text
syntaxmesh init --verified-history
```

### StateChronicle records graph generations, not individual hot-path facts

Do NOT write every node/edge mutation independently into StateChronicle.

The useful boundary is the accepted graph generation.

```text
Turso transaction
      |
      v
canonical generation N+1
      |
      v
graph root / generation manifest
      |
      v
StateChronicle transition
      |
      v
generation VERIFIED
```

A generation manifest should include enough deterministic identity to verify what was accepted:

```text
repository identity
Git commit / working-tree generation
previous generation
new generation
graph root
changed-file set/root
node/edge delta root
SyntaxMesh version
store schema version
extractor versions
Tree-sitter grammar versions
resolver versions
configuration fingerprint
timestamp / logical sequence
```

StateChronicle enables:

- tamper-evident graph history
- deterministic generation verification
- signed CI analysis artifacts
- reproducible architecture-policy results
- verifiable snapshot provenance
- auditability for security/compliance workflows
- proof that a report corresponds to a specific accepted graph generation

Expose states clearly:

```text
DURABLE
    committed to canonical Turso state

VERIFIED
    canonical generation additionally recorded/verified through StateChronicle
```

The graph remains fully usable in `DURABLE` state.

### Important invariant

Turning StateChronicle on or off MUST NOT alter:

- stable node IDs
- stable edge IDs
- graph semantics
- Turso schema semantics
- resolver behavior
- graph query results
- snapshot logical contents

StateChronicle adds verification, not another graph model.

This ensures existing repositories can enable verified history later without rebuilding the conceptual graph format.

---

## Combined responsibility model

```text
Turso
= current canonical source-intelligence state

Rust graph engine
= fast graph execution

Penelope
= reliable transition from one durable state/generation to another

StateChronicle
= optional cryptographic verification of accepted generation history

DuckDB
= historical and analytical computation across generations
```

Keep these responsibilities separate.

---

# 42.5 General-purpose product surfaces

SyntaxMesh MUST remain useful with no agent or LLM installed.

First-class usage surfaces:

## CLI / terminal

```text
syntaxmesh search
syntaxmesh callers
syntaxmesh impact
syntaxmesh path
syntaxmesh diff
syntaxmesh architecture check
```

## IDE/editor integration

Provide an API usable for:

- find references
- call hierarchy
- dependency navigation
- blast radius before rename/refactor
- architecture-boundary diagnostics
- related tests
- related config/schema/API entities
- graph-backed code lens
- historical context

Do not try to replace LSP. Integrate with it and add repository-level knowledge that ordinary language servers do not provide.

## CI / code review

Use graph diffs to provide:

- changed public API
- affected downstream components
- tests that should run
- new dependency cycles
- architecture-rule violations
- schema/API impact
- ownership/reviewer hints
- high-blast-radius changes

## Architecture / platform engineering

Use SyntaxMesh as a continuously maintained architecture model:

- dependency maps
- layer enforcement
- service/package boundaries
- architecture drift
- cross-repository contracts
- coupling trends
- migration planning

## Security / compliance tooling

Enable deterministic queries such as:

- where secret/config values flow
- which endpoints reach sensitive data
- where auth checks occur
- which modules depend on vulnerable packages
- which code paths reach privileged operations

SyntaxMesh supplies graph facts and evidence; specialized security tools can build policy on top.

## Documentation and developer portals

Generate current, evidence-backed views for:

- module relationships
- API-to-handler-to-storage paths
- ownership
- rationale/ADR links
- dependency diagrams
- onboarding maps

## Refactoring and migration tooling

Expose primitives for:

- rename/move impact
- package extraction
- dependency inversion planning
- dead-code candidates
- interface migration
- database/schema migration impact
- monolith decomposition analysis

## Repository analytics

DuckDB enables:

- dependency growth
- hotspot evolution
- coupling trends
- ownership concentration
- architecture drift
- change-frequency correlations
- historical graph comparisons

## Agents

Agents consume the same stable query primitives through MCP/HTTP.

They are one client class among many.

---

# 43. Agent UX

Design tools around questions agents actually ask:

```text
Where is X defined?

Who calls X?

What does X call?

What changes if X changes?

Why does A depend on B?

Show path A -> B.

Which tests cover X?

What DB table does this endpoint reach?

What architectural rules affect this file?

What changed structurally in this branch?

Give me enough context to safely edit X.
```

These should be fast, deterministic primitives.

---

# 44. Improvement over Graphify: no monolithic `graph.json` authority

Graphify's portable JSON model is simple and useful for interchange, but it becomes a scaling and synchronization liability when the graph is live.

SyntaxMesh improvement:

```text
Graphify-like:
AST -> dicts -> NetworkX -> graph.json -> reload/query

SyntaxMesh:
AST -> typed facts -> transactional DB -> incremental projection -> query
```

Keep JSON as an export, not the operational state.

---

# 45. Improvement: hot graph state by construction

A persistent MCP server should not need a special reload because an external command rewrote a JSON file.

The daemon owns live canonical state.

Updates advance index generation.

Readers automatically use a valid generation.

---

# 46. Improvement: eliminate graph-file write races

Multiple hooks/commits/processes should communicate with one writer/coordinator rather than race to overwrite one graph file.

Use:

- daemon IPC
- DB transactions
- advisory process lock if needed
- generation IDs

---

# 47. Improvement: no fragile filesystem checkpoint mesh

Incremental state belongs in canonical transactional tables.

Do not scatter:

```text
graph.json
manifest.json
analysis.json
cache/*
checkpoint files
```

such that their versions can disagree.

Use database generations and transaction boundaries.

---

# 48. Improvement: complete producer identity

Every build/index generation records:

```text
syntaxmesh version
schema version
language-plugin versions
Tree-sitter grammar versions
resolver versions
semantic model/provider/revision
prompt fingerprint
config fingerprint
Git commit
working-tree hash
timestamp
```

That makes experiments reproducible.

---

# 49. Improvement: typed graph semantics

Avoid arbitrary relation strings as the main internal representation.

Use an extensible enum/registry.

Core examples:

```text
CONTAINS
DEFINES
CALLS
IMPORTS
EXPORTS
IMPLEMENTS
EXTENDS
READS
WRITES
REFERENCES
CONFIGURES
ROUTES_TO
QUERIES
COVERS
DEPENDS_ON
DOCUMENTS
EXPLAINS
MOTIVATES
OBSERVED_CALL
```

Plugins can add namespaced relations.

---

# 50. Improvement: distinguish structural graph from knowledge graph

Do not put all relationships into one undifferentiated layer.

Potential layers:

```text
syntax graph
semantic code graph
build graph
data/schema graph
runtime graph
documentation/rationale graph
historical graph
agent-memory graph
```

Queries can select layers.

This prevents a semantically similar documentation edge from behaving like a real function call.

---

# 51. Improvement: confidence is not provenance

Do not treat a single numeric confidence score as sufficient.

Store:

- derivation type
- evidence
- confidence if probabilistic
- extractor identity
- resolution strategy

A deterministic unresolved import and an LLM-inferred relationship are fundamentally different categories.

---

# 52. Improvement: graph-aware invalidation

When a public symbol changes, SyntaxMesh should know which references require re-resolution.

This is better than cache invalidation based purely on source-file content hashes.

---

# 53. Improvement: query planner

An agent question should not default to global graph search.

Classify intent.

Examples:

```text
"who calls X?"              -> reverse CALLS traversal
"what breaks if X changes?" -> impact query
"why A -> B?"               -> path + provenance
"where is auth?"            -> hybrid symbol/text search
"how request reaches DB?"   -> constrained path search
```

Later use an optional LLM to translate natural language into query ops, but keep execution deterministic.

---

# 54. Improvement: answer with evidence

Every agent-facing structural claim should be able to include:

```text
file
line range
symbol
relation
derivation
graph generation
```

This lets the coding model inspect the relevant code instead of trusting an opaque summary.

---

# 55. Improvement: token-budgeted evidence packs

Build a dedicated context optimizer.

Measure value per token.

Potential score:

```text
relevance
* evidence quality
* graph proximity
* uniqueness
/ token_cost
```

Avoid sending duplicate source snippets or repeated signatures.

---

# 56. Improvement: branch/worktree awareness

Modern AI coding routinely uses worktrees and parallel agents.

SyntaxMesh should treat this as a first-class scenario.

Each agent should query the graph corresponding to its worktree, not accidentally main.

---

# 57. Improvement: multi-repository graphs

Support a workspace graph eventually:

```text
frontend repo
backend repo
SDK repo
infra repo
schema repo
```

Cross-repo edges can represent:

- HTTP API calls
- package dependencies
- protobuf/OpenAPI contracts
- events/topics
- database schemas
- artifacts

Do not make multi-repo mandatory for v0.1.

---

# 58. Improvement: historical analytics with DuckDB

Graphify-style point-in-time graph generation is useful.

SyntaxMesh should additionally make historical change cheap.

Use DuckDB to make time a normal analytical dimension.

---

# 59. Improvement: benchmark performance and usefulness separately

Two benchmark families are required.

## Engine benchmarks

Measure:

- full scan speed
- incremental scan speed
- memory usage
- DB size
- graph projection build time
- query latency
- snapshot export/import
- concurrent reader performance
- SQLite writer throughput
- Turso concurrent writer throughput
- DuckDB analytical query time

## Agent usefulness benchmarks

Controlled A/B:

```text
agent + normal grep/read
vs
agent + SyntaxMesh
vs
agent + Graphify where possible
```

Metrics:

- task success
- tokens
- turns
- wall-clock
- source files opened
- hallucinated dependencies
- regressions
- context precision
- context recall

Graphify already publishes a benchmark harness/results, so use its methodology as a useful external comparison while keeping SyntaxMesh's benchmark harness reproducible.

---

# 60. Benchmark datasets

Include:

- small repo
- medium repo
- monorepo
- multi-language repo
- generated-code-heavy repo
- extremely large repo
- branch with small change
- rename-heavy refactor
- architecture-cycle introduction
- API-to-database trace task

Real open-source targets should be pinned to exact commits.

---

# 61. Performance targets

Do not promise numbers before measurements.

Define budgets after baseline measurements.

Candidate goals to validate:

- unchanged re-index should approach filesystem/hash-check cost
- one-file edit should not require whole graph rebuild
- common symbol lookup should be single-digit milliseconds locally
- one-hop neighbor queries should be extremely cheap
- MCP graph generation should remain live during indexing
- memory should scale with active projection rather than raw JSON object overhead

---

# 62. Testing strategy

## Unit

- IDs
- path normalization
- extractors
- resolvers
- graph deltas
- query planning
- ranking
- context budgeting

## Integration

- repository indexing
- incremental update
- delete file
- rename file
- symbol rename
- broken syntax
- generated files
- worktree
- multiple CLI processes
- daemon restart
- database migration

## Storage conformance

One shared suite:

```text
GraphStoreConformance
```

Run against:

- SQLite
- Turso

Tests:

- atomicity
- rollback
- read consistency
- constraint behavior
- pagination
- FTS/search semantics
- migration
- retry handling
- corruption/error paths

## Property/fuzz tests

- parser output boundaries
- path normalization
- snapshot serialization
- graph delta application
- migration
- query parser/DSL
- malformed MCP input

## Crash tests

Kill process:

- during parse
- during DB transaction
- during snapshot export
- during DuckDB sync
- during migration

Canonical live graph must remain valid.

---

# 63. Distribution

Target:

```text
cargo install syntaxmesh
```

Also publish binaries:

- Linux x86_64
- Linux aarch64
- macOS x86_64
- macOS aarch64
- Windows x86_64

Later:

- Homebrew
- Scoop/winget
- Nix
- Docker where useful

Core should not require Docker.

---

# 64. CLI sketch

```text
syntaxmesh init
syntaxmesh index .
syntaxmesh watch
syntaxmesh status
syntaxmesh doctor

syntaxmesh search "AuthService"
syntaxmesh node AuthService
syntaxmesh callers AuthService
syntaxmesh callees AuthService
syntaxmesh impact AuthService
syntaxmesh path HttpHandler UserTable
syntaxmesh explain <fact>

syntaxmesh diff HEAD~1 HEAD
syntaxmesh rules check
syntaxmesh context "how does auth reach postgres" --max-tokens 6000

syntaxmesh snapshot export
syntaxmesh snapshot import

syntaxmesh analytics sync
syntaxmesh analytics sql "..."
```

Keep normal usage simple.

---

# 65. API versioning

Version:

- DB schema
- snapshot format
- MCP tools
- HTTP API
- plugin SDK
- analytical schema

Do not tie all versions blindly to crate semver.

Example:

```text
syntaxmesh binary 0.3
store schema 4
snapshot format 2
plugin ABI/API 1
```

---

# 66. Licensing / clean implementation

Graphify's public repository currently advertises Apache-2.0 and MIT licensing.

Still:

- build SyntaxMesh as its own architecture
- do not copy branding
- do not imply official Graphify compatibility
- if code is copied, comply with the exact upstream license/notice requirements
- preserve attribution where required
- document third-party Tree-sitter grammar licenses
- generate dependency license report in releases

Prefer implementing concepts from first principles rather than mechanically translating Python files to Rust.

Additional licensing requirement:

- [ ] The SyntaxMesh license must permit closed-source embedding and private extension development.
- [ ] Prefer `MIT OR Apache-2.0` unless dependency/legal review identifies a concrete reason to change.
- [ ] Public first-party integrations inherit compatible open-source licensing.
- [ ] Private STEXS platform extensions remain separate proprietary works and must not be required to build/use public SyntaxMesh.


---



# 66.4 Runtime-agnostic embedding architecture

SyntaxMesh must not be architecturally synonymous with `syntaxmeshd`.

The daemon is one executable host around a reusable engine.

The actual product layering should be:

```text
syntaxmesh-core
    domain types
    graph semantics
    stable IDs
    provenance model
    query DTOs
          |
          v
syntaxmesh-engine
    indexing
    graph updates
    runtime ingestion
    query planning
    projections
    application services
          |
    +-----+----------------+----------------+
    |                      |                |
    v                      v                v
syntaxmeshd          embedded Rust API    CI/CLI host
```

The same logical engine should be usable:

- as a standalone daemon
- embedded directly in the STEXS multiplayer platform
- embedded in inventory/economy/control-plane services
- embedded in internal developer-platform components
- in CI
- in local developer tooling

The host decides transport/lifecycle.

The engine owns intelligence semantics.

---

## 66.4.1 Full engine vs thin runtime probe

Support two fundamentally different integration profiles.

### Full engine

For trusted Rust services and tooling:

```text
host process
|
+-- SyntaxMesh engine
+-- Turso
+-- graph execution projection
+-- Penelope
+-- optional StateChronicle verification
+-- optional DuckDB analytics
```

Appropriate hosts:

```text
syntaxmeshd
STEXS control plane
inventory/economy service
architecture service
internal engineering platform
CI worker
IDE backend
```

### Thin probe

For game engines, SDKs, clients, or processes that should not carry the full intelligence stack:

```text
game / SDK / runtime
        |
        +-- tiny SyntaxMesh probe
                |
                v
        typed runtime observations
                |
                v
        canonical SyntaxMesh engine
```

A thin probe MUST NOT require:

```text
Turso
DuckDB
Penelope
StateChronicle
graph algorithms
repository indexing
```

It should contain only:

- typed observation DTOs
- bounded buffering
- serialization/transport
- correlation identifiers
- feature/config gating
- privacy/redaction controls

This keeps shipped clients small and prevents SyntaxMesh from becoming a gameplay runtime dependency.

---

## 66.4.2 Runtime observation protocol

Introduce:

```text
syntaxmesh-runtime-protocol
```

This crate defines language/runtime-neutral observation records.

Conceptual model:

```rust
RuntimeObservation {
    workspace_id,
    producer_id,
    environment,
    timestamp,
    trace_id,
    correlation_id,
    kind,
    subject,
    target,
    attributes,
    provenance,
}
```

Initial observation kinds may include:

```text
MESSAGE_SENT
MESSAGE_RECEIVED
RPC_CALLED
RPC_HANDLED
WORKFLOW_STARTED
WORKFLOW_STEP
RESOURCE_MUTATED
AUTHORIZATION_CHECKED
SERVICE_CALLED
EVENT_PUBLISHED
EVENT_CONSUMED
SYSTEM_EXECUTED
COMPONENT_READ
COMPONENT_WRITTEN
```

The wire representation should be transport-neutral.

Do not make the runtime protocol depend on HTTP specifically.

Potential transports:

```text
in-process Rust channel
Unix/domain socket
HTTP streaming
gRPC adapter
OpenTelemetry adapter
batch file/NDJSON
message-bus adapter
```

---

## 66.4.3 Static truth and runtime truth are separate graph layers

Do not overwrite deterministic static relationships with observed runtime relationships.

Example:

```text
STATIC:
PurchaseSystem
  --SENDS-->
PurchaseItemRequest

RUNTIME:
PurchaseSystem
  --OBSERVED_SEND-->
PurchaseItemRequest
```

Static analysis answers:

> What can/should happen according to source and configuration?

Runtime observations answer:

> What has actually happened in a specific environment?

Both can coexist and be queried together.

Potential provenance classes:

```text
SOURCE_FACT
BUILD_FACT
STATICALLY_RESOLVED
SERVER_RUNTIME_OBSERVED
CLIENT_RUNTIME_OBSERVED
PENELOPE_OBSERVED
STATECHRONICLE_COMMITTED
HEURISTIC
SEMANTIC_INFERENCE
```

Do not assign equal trust to every class.

---

## 66.4.4 Runtime trust hierarchy

Runtime facts need trust semantics.

Example ordering for many system-analysis use cases:

```text
STATECHRONICLE_COMMITTED
    strongest authoritative state evidence

PENELOPE_OBSERVED
    authoritative workflow execution evidence

SERVER_RUNTIME_OBSERVED
    trusted service-side observation

CLIENT_RUNTIME_OBSERVED
    untrusted/weak evidence

HEURISTIC
    derived/non-authoritative
```

Do not let a game client assert authoritative server state merely by emitting a probe event.

Example:

```text
client:
"I mutated PlayerBalance"
```

must not become equivalent to:

```text
StateChronicle:
committed PlayerBalance transition
```

Graph/query APIs should allow callers to filter by evidence/trust class.

---

## 66.4.5 Runtime structural evidence, not telemetry warehousing

SyntaxMesh must not become Prometheus, ClickHouse, an OpenTelemetry collector, or a raw event warehouse.

If:

```text
Gateway -> InventoryService
```

occurs fifty million times, SyntaxMesh generally needs one structural relation with aggregate observation metadata, not fifty million permanent graph edges.

Example:

```text
Gateway
  --OBSERVED_CALL-->
InventoryService

metadata:
  first_seen
  last_seen
  observation_count
  environment
  selected latency/error summaries if useful
```

High-volume raw telemetry stays in the actual observability platform.

Preferred flow:

```text
raw runtime telemetry
       |
       v
adapter / aggregation / dedupe
       |
       v
structural RuntimeObservation
       |
       v
SyntaxMesh
```

DuckDB may receive historical aggregates where useful.

---

## 66.4.6 Development vs production runtime collection

Runtime collection should be configurable.

Development builds can collect rich topology:

```text
registered systems
plugins
resources
message flows
workflow events
state mutations
authorization paths
```

Production clients should default to minimal safe collection.

Example configuration:

```toml
[syntaxmesh.runtime]
enabled = true
mode = "structural"
sample_flows = true
sample_rate = 0.01
```

Server-side authoritative integrations may collect more than client-side probes.

No hidden network transmission.

---

## 66.4.7 Embeddable Rust API

Provide a stable application API independent of daemon transports.

Conceptually:

```rust
let mesh = SyntaxMesh::builder()
    .workspace(workspace)
    .store(store)
    .build()
    .await?;

mesh.ingest_source(change).await?;
mesh.ingest_runtime(observation).await?;

let result = mesh
    .query()
    .impact(selector)
    .await?;
```

The exact API should avoid forcing one async runtime into `syntaxmesh-core`.

Runtime-specific async integration belongs in engine/adapter crates.

Anything possible through `syntaxmeshd` should ultimately delegate to this engine/application API rather than implementing unique graph semantics inside the daemon.

---

## 66.4.8 Revised crate boundaries

Recommended additions/revisions:

```text
syntaxmesh-core
    pure domain model

syntaxmesh-api-model
    public DTOs

syntaxmesh-engine
    embeddable application/intelligence engine

syntaxmesh-runtime-protocol
    lightweight runtime observation contract

syntaxmesh-runtime
    runtime ingestion/aggregation

syntaxmesh-extension-sdk
    stable extension contracts / manifests / registration

syntaxmesh-extension-protocol
    out-of-process extension and fact-ingestion protocol

syntaxmesh-store
syntaxmesh-store-turso
syntaxmesh-store-sqlite

syntaxmesh-graph
syntaxmesh-query
syntaxmesh-analysis

syntaxmesh-integration-penelope
syntaxmesh-integration-statechronicle
syntaxmesh-integration-trustgrant
syntaxmesh-integration-shardline

syntaxmesh-framework-bevy
syntaxmesh-framework-stexs-bevy

syntaxmeshd
    standalone host only

syntaxmesh-cli
syntaxmesh-http
syntaxmesh-mcp
```

Dependency direction must keep:

```text
syntaxmesh-core
syntaxmesh-api-model
syntaxmesh-runtime-protocol
```

free from heavy runtime/service dependencies.

---

## 66.4.9 Penelope integration in the runtime-agnostic model

Penelope remains mandatory for the **full SyntaxMesh engine** whenever operations cross durable/multi-step boundaries.

Examples:

```text
repository indexing generations
large resolver rebuild
workspace bootstrap
DuckDB synchronization
snapshot publication
StateChronicle publication
migration
recovery
long-running CI analysis
```

But Penelope MUST NOT be required by:

```text
syntaxmesh-core
syntaxmesh-api-model
syntaxmesh-runtime-protocol
thin Bevy/Unity/Unreal/SDK probes
```

Otherwise a tiny runtime emitter would drag the entire durable workflow stack into every client.

Penelope has a second role: it is also an analysis/runtime fact source.

A Penelope adapter can expose workflow structure and observed executions:

```text
PurchaseSaga
  --HAS_STEP-->
ReserveFunds

PurchaseSaga
  --HAS_STEP-->
GrantItem

InventoryHandler
  --STARTS_WORKFLOW-->
PurchaseSaga
```

Runtime:

```text
PurchaseSaga
  --OBSERVED_STEP-->
GrantItem
```

Correlate via trace/workflow IDs where possible.

Do not recursively run every Penelope observation through another Penelope saga.

Hot observation ingestion remains batched and cheap.

---

## 66.4.10 StateChronicle integration in the runtime-agnostic model

StateChronicle continues to serve two separate purposes.

### A. Verify SyntaxMesh graph-generation history

As already defined:

```text
graph generation
      |
      v
StateChronicle
      |
      v
verified generation history
```

This remains runtime opt-in.

### B. Supply authoritative application state evidence

When SyntaxMesh analyzes a system using StateChronicle, committed transitions become high-quality graph evidence.

Example:

```text
PurchaseWorkflow
  --MUTATES_RESOURCE-->
PlayerInventory
  provenance = SOURCE_FACT
```

plus:

```text
PurchaseWorkflow
  --OBSERVED_MUTATION-->
PlayerInventory
  provenance = STATECHRONICLE_COMMITTED
```

Useful queries:

```text
Which declared mutation paths have actually occurred?

Which state transitions occur in production but are absent from
the architecture model?

Which workflows have committed changes to PlayerInventory?

Which externally reachable flows have ever resulted in a
CurrencyBalance mutation?
```

SyntaxMesh MUST NOT become a StateChronicle execution engine.

---

## 66.4.11 TrustGrant in the runtime-agnostic model

TrustGrant can similarly contribute both static and runtime authority facts.

Static:

```text
PurchaseHandler
  --REQUIRES_CAPABILITY-->
inventory.purchase
```

Runtime:

```text
PurchaseWorkflow
  --OBSERVED_AUTHORIZATION-->
inventory.purchase
```

Correlating authority, workflow, and state facts gives:

```text
request
  -> authorization
  -> workflow
  -> committed state transition
```

This enables stronger security analysis without moving authorization responsibility into SyntaxMesh.

---

## 66.4.12 Full STEXS transaction path

A representative end-to-end graph can become:

```text
Bevy ShopUiSystem
       |
       | SENDS
       v
PurchaseItemRequest
       |
       v
STEXS Gateway
       |
       v
InventoryHandler
       |
       | REQUIRES_CAPABILITY
       v
inventory.purchase                    TrustGrant
       |
       | STARTS_WORKFLOW
       v
PurchaseSaga                         Penelope
       |
       +--> DebitCurrency
       |
       +--> GrantItem
               |
               | MUTATES_RESOURCE
               v
         PlayerInventory             StateChronicle
```

The same path can contain multiple evidence dimensions:

```text
static source relation
runtime observed relation
workflow observation
authorization observation
state-committed observation
```

This is a major architectural goal.

---

## 66.4.13 Game-engine integration

Game-engine SDKs should implement thin probes, not full embedded graph engines by default.

For `stexs-bevy`, the probe can understand:

```text
System
Plugin
Resource
Component
Event
Schedule
NetworkMessage
```

and emit runtime facts such as:

```text
SYSTEM_EXECUTED
MESSAGE_SENT
MESSAGE_RECEIVED
RESOURCE_READ
RESOURCE_WRITTEN
```

A development/editor environment MAY embed a full SyntaxMesh engine if useful.

A production game binary SHOULD normally use the thin probe or no runtime probe at all.

Future SDKs for Unity/Unreal/etc. should share the same runtime observation contract.

---

## 66.4.14 Host equivalence

The following should all delegate to the same engine semantics:

```text
syntaxmeshd
embedded backend integration
CI host
CLI
HTTP service
MCP
IDE backend
```

Avoid:

```text
daemon-only features
CLI-only graph semantics
MCP-only query implementations
```

All adapters should translate to common application/query operations.

---


## 66.4.15 Extensible system intelligence

SyntaxMesh must not understand STEXS systems because STEXS concepts are hard-coded into the core.

STEXS integrations should dogfood the same extension architecture available to every user.

The intended model:

```text
                         SyntaxMesh Core
                              |
                    stable graph / extension API
                              |
        +---------------------+----------------------+
        |                     |                      |
        v                     v                      v
 source analyzers       system adapters       runtime observers
        |                     |                      |
        v                     v                      v
 Rust / Python /       user infrastructure     user runtimes /
 TS / config formats   and platforms           telemetry sources
```

The core owns:

- stable IDs
- generic nodes/edges/facts
- provenance
- graph storage/execution
- query semantics
- extension registration
- namespace validation
- ingestion contracts

Extensions own domain meaning.

---

### 66.4.15.1 Extension classes

Support multiple extension classes instead of treating everything as a language plugin.

#### Source analyzers

Transform source/configuration artifacts into facts.

Examples:

```text
Rust
Python
TypeScript
SQL
OpenAPI
protobuf
GraphQL
Terraform
custom DSL
```

Generic language/config analyzers can be part of SyntaxMesh itself when they are fundamental source-intelligence capabilities rather than vendor/domain integrations.

#### System adapters

Model platform/infrastructure semantics.

Examples external users could build:

```text
company-specific deployment platform
custom message router
internal billing system
proprietary workflow engine
custom database abstraction
```

Public SyntaxMesh does not need to ship these.

#### Runtime observers

Translate runtime events into `RuntimeObservation`.

Examples:

```text
custom game runtime
proprietary backend framework
internal event bus
custom workflow engine
```

#### Policy/analysis extensions

Supply domain-specific rules and analyses over the common graph.

Examples:

```text
"EU services may only write EU data stores"

"payment workloads must pass through authorization gateway"

"game data-plane services cannot call control-plane admin APIs"
```

#### Export/consumer adapters

Consume public graph DTOs/NDJSON for:

```text
visualization
reporting
IDE integration
internal developer portals
custom CI systems
```

---

### 66.4.15.2 Namespaced semantics

Do not grow one global enum containing every domain concept ever invented.

Core relationships remain small and general:

```text
core:CONTAINS
core:REFERENCES
core:DEPENDS_ON
core:CALLS
core:READS
core:WRITES
core:PRODUCES
core:CONSUMES
```

Extensions register namespaced concepts:

```text
penelope:WORKFLOW
penelope:STARTS_WORKFLOW

statechronicle:RESOURCE
statechronicle:MUTATES_RESOURCE

trustgrant:CAPABILITY
trustgrant:REQUIRES_CAPABILITY

bevy:SYSTEM
bevy:WRITES_RESOURCE

acme:FRAUD_PIPELINE
acme:ROUTES_PAYMENT
```

The graph engine understands graph mechanics.

The extension understands the domain semantics.

---

### 66.4.15.3 Extension manifest

Every extension should have explicit identity and capabilities.

Conceptual manifest:

```toml
[extension]
id = "acme.payment-platform"
version = "1.2.0"
api_version = 1

[provides]
node_kinds = [
    "acme:payment_service",
    "acme:ledger"
]

relations = [
    "acme:SETTLES_THROUGH",
    "acme:ROUTES_PAYMENT"
]

[capabilities]
static_analysis = true
runtime_observations = true
policies = true
```

Every produced fact records:

```text
extension ID
extension version
extension API version
producer configuration fingerprint
```

This is required for:

- reproducibility
- cache invalidation
- debugging
- migration
- provenance
- deterministic re-indexing

---

### 66.4.15.4 In-process Rust extension API

Trusted Rust hosts should be able to inject extensions directly.

Conceptually:

```rust
pub trait SyntaxMeshExtension: Send + Sync {
    fn manifest(&self) -> &ExtensionManifest;

    fn register(
        &self,
        registry: &mut ExtensionRegistry,
    ) -> Result<(), ExtensionError>;
}
```

Specialized traits may include:

```rust
SourceAnalyzer
RuntimeObserver
FactProducer
PolicyProvider
QueryExtension
```

A closed-source host can compile private integration crates together with the open-source SyntaxMesh engine.

Example:

```rust
let mesh = SyntaxMesh::builder()
    .extension(PenelopeExtension::new(...))
    .extension(StateChronicleExtension::new(...))
    .extension(InternalMatchmakingExtension::new(...))
    .extension(InternalInventoryExtension::new(...))
    .build()
    .await?;
```

The private crates need not be upstreamed or exposed publicly.

Do not rely on unstable Rust `dylib` ABI as the primary plugin mechanism.

Compile-time injection through public traits is the preferred path for trusted Rust systems.

---

### 66.4.15.5 Out-of-process extension protocol

Do not require every SyntaxMesh extension to be written in Rust.

Provide a stable external protocol for producers implemented in:

```text
Go
Python
C#
Java/Kotlin
TypeScript
C++
other languages
```

External adapters emit validated namespaced facts/runtime observations into SyntaxMesh.

Possible transports:

```text
NDJSON stream
framed local IPC
HTTP streaming
gRPC adapter
```

The transport is secondary.

The logical extension/fact protocol is stable.

This is particularly important for:

```text
Unity/C#
Unreal/C++
JVM infrastructure
proprietary enterprise platforms
```

---

### 66.4.15.6 Core typed envelope, extension-defined payload

Do not reduce the entire extension system to:

```text
HashMap<String, serde_json::Value>
```

Use a typed common envelope.

Conceptually:

```rust
pub struct Fact {
    pub id: StableFactId,
    pub namespace: NamespaceId,
    pub subject: EntityRef,
    pub predicate: QualifiedRelation,
    pub object: FactObject,
    pub provenance: Provenance,
    pub extension_data: ExtensionData,
}
```

Core fields required for indexing/query/provenance remain typed.

Extensions get a versioned payload for domain-specific attributes.

---

### 66.4.15.7 Extension isolation and failure semantics

An extension must not be able to silently corrupt canonical graph state.

Requirements:

- validate manifests before registration
- namespace ownership checks
- transaction boundary around extension-produced fact batches
- schema/version validation
- bounded resources
- explicit diagnostics
- extension-specific provenance
- failure isolation
- no hidden network calls
- deterministic static producers where applicable

An optional extension failure should degrade that extension's facts, not invalidate unrelated graph layers.

Mandatory host integrations may choose fail-closed behavior.

---

### 66.4.15.8 User-owned proprietary infrastructure

A primary design goal is allowing users to model systems SyntaxMesh has never heard of.

Example user architecture:

```text
React frontend
      |
      v
GraphQL gateway
      |
      v
proprietary workflow platform
      |
      v
internal event router
      |
      v
custom ledger
```

The company can implement private SyntaxMesh extensions for:

```text
workflow nodes
event-router topics
ledger resources
authorization rules
runtime observations
deployment topology
```

without:

- forking SyntaxMesh
- upstreaming proprietary code
- exposing internal schemas
- changing `syntaxmesh-core`

If private integrations repeatedly require core patches, the extension architecture is insufficient and should be improved.

---

### 66.4.15.9 Extension API stability

Treat the extension SDK as a real public API.

Version separately:

```text
SyntaxMesh binary version
storage schema version
snapshot/NDJSON schema version
extension API version
runtime observation protocol version
```

Provide:

- compatibility matrix
- semver guarantees after stabilization
- compile-time API checks
- golden protocol fixtures
- extension conformance tests
- migration/deprecation policy

This is required before encouraging a third-party extension ecosystem.

---


# 66.5 Cloud deployment and STEXS interoperability

SyntaxMesh is **local-first**, not **laptop-only**.

The same engine should work in:

```text
developer workstation
CI runner
build farm
internal Kubernetes cluster
self-hosted engineering platform
STEXS multiplayer backend control plane
```

without requiring a hosted SyntaxMesh service, Turso Cloud, or any external database.

The core deployment rule is:

> Move the SyntaxMesh process to the cloud when needed; do not move the canonical embedded database into a mandatory cloud dependency.

---

## 66.5.1 Deployment boundary: engineering/control plane, not gameplay data plane

For multiplayer systems, SyntaxMesh MUST remain outside latency-sensitive gameplay execution.

Do not place SyntaxMesh in paths such as:

```text
player packet
  -> gateway
  -> authoritative simulation
  -> replication
```

SyntaxMesh belongs in the engineering/control plane:

```text
                         GAME / DATA PLANE
                +-------------------------------+
                | realtime gateways             |
                | authoritative match servers   |
                | matchmaking                   |
                | state/economy services        |
                | presence/chat                 |
                | replication                   |
                +-------------------------------+
                               ^
                               |
                    contracts / deployments
                               |
                +-------------------------------+
                | ENGINEERING / CONTROL PLANE   |
                |                               |
                | source repositories           |
                | CI/CD                         |
                | architecture policy           |
                | dependency analysis           |
                | API/schema analysis           |
                | release impact                |
                | security analysis             |
                |         SyntaxMesh            |
                +-------------------------------+
```

SyntaxMesh may analyze the game/data plane, but must never become a runtime dependency for a match to execute.

---

## 66.5.2 Supported operating modes

### Mode A — developer/local

```text
developer machine
      |
      +-- syntaxmeshd
             |
             +-- embedded Turso
             +-- graph projection
             +-- Penelope
             +-- optional StateChronicle
             +-- DuckDB
```

Properties:

- zero external services
- repository/worktree aware
- local HTTP/IPC/MCP access
- continuous filesystem indexing
- local analytical history

### Mode B — ephemeral CI

```text
CI runner
   |
   +-- checkout
   +-- restore optional immutable SyntaxMesh bootstrap artifact
   +-- run SyntaxMesh
   +-- index delta
   +-- evaluate impact/rules/tests
   +-- emit NDJSON/SARIF/report artifacts
   +-- exit
```

The CI mode should not need a permanently running database.

Primary use cases:

- changed-symbol analysis
- selective test/build planning
- architecture rules
- graph diff
- security policy
- API/schema impact
- PR annotations

### Mode C — persistent self-hosted cloud workspace

```text
internal load balancer / router
              |
        workspace routing
        /       |       \
       v        v        v
 workspace A workspace B workspace C
       |        |        |
 syntaxmeshd syntaxmeshd syntaxmeshd
       |        |        |
 local       local      local
 Turso       Turso      Turso
```

Each workspace runtime owns its local embedded state.

This mode is for:

- teams
- large repositories
- cross-repository workspaces
- long-lived history
- internal developer portals
- IDE/CI consumers
- architecture/security services
- multiplayer backend engineering systems

No Turso Cloud is involved.

---

## 66.5.3 Kubernetes / container deployment

A persistent cloud instance should look conceptually like:

```text
Pod / container
+-- syntaxmeshd
+-- local/attached persistent volume
    +-- graph.db
    +-- analytics.duckdb
    +-- local workflow state
```

Rules:

- [ ] One owning SyntaxMesh daemon per live workspace database.
- [ ] Use a persistent volume or host-local durable disk where persistence is needed.
- [ ] Do not rely on multiple pods concurrently opening the same embedded database through NFS/shared network filesystems.
- [ ] Horizontal scaling occurs by **workspace/repository partitioning**, not by many processes sharing one DB file.
- [ ] A workspace can be reassigned to another worker by restoring a snapshot/bootstrap artifact and replaying/reindexing the delta.
- [ ] Health/readiness endpoints must expose index generation and recovery status.
- [ ] Graceful shutdown must let Penelope checkpoint/reconcile durable work.
- [ ] StateChronicle verification, if enabled, must survive worker replacement independently of hot graph projection state.

---

## 66.5.4 Workspace as the cloud ownership unit

Introduce a first-class:

```text
WorkspaceId
```

A workspace may contain:

```text
one repository
multiple repositories
multiple worktrees
shared protocol/schema repositories
SDK repositories
infrastructure repositories
```

Example:

```text
STEXS multiplayer workspace
|
+-- multiplayer-backend
+-- stexs-bevy
+-- protocol/schema repo
+-- SDK repo
+-- deployment/infra repo
```

The workspace owns:

- Turso canonical graph state
- graph generation sequence
- Penelope workflow scope
- optional StateChronicle generation history
- DuckDB analytics
- repository mappings
- graph-layer configuration
- architecture rules

This becomes the natural isolation boundary for multi-team or multi-project cloud installations.

---

## 66.5.5 Cross-repository graph

Cloud/backend projects frequently split responsibilities across repositories.

SyntaxMesh should support graph edges across repository boundaries.

Examples:

```text
Bevy client message
    -> protocol definition
    -> gateway decoder
    -> backend handler
    -> service
    -> StateChronicle resource
```

or:

```text
OpenAPI operation
    -> generated SDK method
    -> game client integration
```

Cross-repository relationships may come from:

- package dependencies
- protocol/schema identity
- OpenAPI
- protobuf
- GraphQL
- event schemas
- message IDs
- crate/package references
- build metadata
- manually declared workspace contracts

Do not couple repository identity to one filesystem root.

---

## 66.5.6 Multiplayer/backend-specific graph entities

SyntaxMesh should remain general-purpose, but its model should be extensible enough for STEXS multiplayer infrastructure.

Potential node kinds:

```text
Service
Endpoint
RpcMethod
ProtocolMessage
Event
Topic
Queue
Database
Table
Migration
Deployment
Container
BuildTarget
ConfigKey
SecretReference
StateResource
Workflow
Capability
BevySystem
BevyResource
BevyEvent
SdkMethod
```

Potential relations:

```text
SENDS
RECEIVES
HANDLES
ROUTES_TO
PUBLISHES
SUBSCRIBES
QUERIES
WRITES
DEPLOYS
REQUIRES_CAPABILITY
ORCHESTRATES
MUTATES_RESOURCE
COVERS
GENERATES
IMPLEMENTS_CONTRACT
OBSERVED_CALL
```

These should live in extensible graph layers rather than hard-coding a "game backend" worldview into the core source model.

---

## 66.5.7 Selective CI / build planning

A major cloud use case is deciding what actually needs to build and test.

Pipeline:

```text
Git diff
   |
   v
changed files/symbols/contracts
   |
   v
SyntaxMesh impact graph
   |
   +--> affected crates/packages
   +--> affected services
   +--> affected tests
   +--> affected SDKs
   +--> affected schemas
   +--> affected deployment units
```

Example result:

```text
changed:
  economy/pricing.rs

build:
  economy
  inventory
  game-sdk

test:
  economy-unit
  inventory-integration
  marketplace-contract

skip:
  matchmaking
  chat
  presence
```

SyntaxMesh only produces facts/plans.

The CI system remains responsible for execution.

---

## 66.5.8 Deployment impact analysis

Expose deployment-aware impact queries.

Example:

```text
syntaxmesh impact \
  --changed-since <commit> \
  --layer deployment
```

Result can stream as NDJSON:

```text
rebuild
redeploy
republish
retest
unaffected
```

A protocol change might imply:

```text
rebuild:
  gateway
  realtime-server
  sdk

redeploy:
  gateway
  realtime-server

publish:
  stexs-sdk
  stexs-bevy
```

SyntaxMesh MUST NOT become the deployment orchestrator itself.

It provides dependency truth to deployment tooling.

---

## 66.5.9 Architecture enforcement for distributed multiplayer systems

Support architecture rules that operate above source-file level.

Examples:

```text
realtime match/data plane
    MUST NOT depend directly on control-plane admin services

gameplay service
    MUST NOT query account database directly

client-facing gateway
    MUST NOT mutate authoritative state without passing
    through the declared state/authorization boundary
```

Example rule concept:

```toml
[[rule]]
name = "gameplay-cannot-call-control-plane"
from = "layer:gameplay"
to = "layer:control-plane"
relations = ["CALLS", "DEPENDS_ON", "ROUTES_TO"]
action = "deny"
```

The rule engine should work with generic layers so non-game projects can express their own architecture.

---

## 66.5.10 Runtime topology enrichment

Static source intelligence can be enriched with optional runtime observations.

Potential source:

```text
OpenTelemetry traces
service mesh telemetry
test traces
profiling/coverage
```

Keep distinct semantics:

```text
STATICALLY_POSSIBLE
OBSERVED_AT_RUNTIME
```

Never replace static relationships with runtime observations.

Example:

```text
Gateway --CALLS--> Matchmaker
Gateway --OBSERVED_CALL--> Matchmaker
```

Potential queries:

```text
Which static dependencies have never been observed?

Which runtime service edges are missing from the static model?

Which services actually participate in the login flow?

Which production paths are affected by this code change?
```

Runtime ingestion should remain optional and adapter-based.

---

# 66.6 STEXS ecosystem interoperability

SyntaxMesh should compose with existing STEXS projects through narrow contracts.

The current STEXS portfolio already has distinct responsibility domains:

```text
Shardline
  content-addressed artifact/content truth

Penelope
  deterministic durable process/workflow execution

StateChronicle
  verifiable resource/state transition truth

TrustGrant
  delegated authority/capability truth

stexs-bevy
  game-engine/client integration

SyntaxMesh
  source/system/architecture intelligence
```

SyntaxMesh must not duplicate those responsibilities.

The integration principle is:

> Understand and consume STEXS facts where useful; do not collapse all STEXS projects into one runtime.

---

## 66.6.1 Penelope interoperability — mandatory internal reliability

Penelope remains SyntaxMesh's required durable workflow layer.

Cloud-specific uses add:

- workspace bootstrap
- repository synchronization
- webhook-triggered indexing
- large cross-repo rebuilds
- CI analysis workflows
- snapshot restore/rebuild
- Shardline artifact publication/retrieval
- DuckDB analytical synchronization
- StateChronicle publication/reconciliation
- runtime-telemetry ingestion jobs

Penelope scopes should include `WorkspaceId` so two workspaces cannot accidentally share workflow identity.

SyntaxMesh must continue to keep Penelope out of hot query/traversal paths.

---

## 66.6.2 StateChronicle interoperability — verified graph generations

StateChronicle remains the opt-in verifier for accepted SyntaxMesh graph generations.

Cloud benefits:

- signed/verifiable CI graph inputs
- verifiable architecture-rule results
- tamper-evident historical generations
- reproducible security analysis
- auditability across worker replacement
- proof that a deployment-impact report was calculated from a specific graph generation

StateChronicle should record generation manifests/roots, not individual node writes.

Potential STEXS-specific graph understanding:

```text
StateChronicle resource
StateChronicle transaction type
state mutation path
ownership/balance/inventory resource
```

SyntaxMesh may discover these relationships statically and represent them as graph facts.

It must not become a StateChronicle execution engine.

---

## 66.6.3 Shardline interoperability — immutable graph/index artifacts

Shardline is a strong fit for **immutable SyntaxMesh artifacts**, not the live Turso database.

Good artifact classes:

```text
deterministic SyntaxMesh snapshots
bootstrap/index snapshots
NDJSON exports
DuckDB Parquet partitions
extractor caches
CI analysis artifacts
benchmark corpora/results
large generated metadata
```

Potential bootstrap flow:

```text
new CI/cloud SyntaxMesh worker
        |
        v
resolve latest compatible snapshot identity
        |
        v
Shardline
        |
        v
fetch immutable snapshot
        |
        v
restore local Turso state
        |
        v
index only repository delta
```

Benefits:

- fast worker cold start
- deduplication across branches/builds
- content-addressed reproducibility
- reduced repeated indexing work
- large artifact distribution without abusing Git

Never:

```text
mount live Turso DB from Shardline
```

Shardline holds immutable artifacts/content; Turso holds current mutable graph state.

Potential future contract:

```text
SyntaxMeshSnapshotManifest {
    format_version
    workspace_id
    source_commit_set
    graph_generation
    extractor_fingerprint
    schema_version
    content_hash
}
```

The manifest itself can be verified by StateChronicle when verified-history mode is enabled.

---

## 66.6.4 TrustGrant interoperability — authority for shared/cloud SyntaxMesh

TrustGrant should remain optional for ordinary local development.

It becomes useful when SyntaxMesh is exposed as a shared internal/cloud service.

Possible protected resources:

```text
workspace
repository
graph layer
source evidence
architecture policy
history
admin operation
snapshot
```

Potential capabilities:

```text
syntaxmesh.workspace.read
syntaxmesh.workspace.index
syntaxmesh.graph.query
syntaxmesh.source.read
syntaxmesh.history.read
syntaxmesh.rules.evaluate
syntaxmesh.rules.manage
syntaxmesh.snapshot.export
syntaxmesh.workspace.admin
```

This allows delegated authority without building a second custom permission protocol into SyntaxMesh.

TrustGrant should authorize SyntaxMesh operations at the service boundary.

It should not be consulted inside every graph edge traversal.

Example:

```text
client request
    |
    v
TrustGrant capability evaluation
    |
    +-- denied -> stop
    |
    v
authorized SyntaxMesh query
    |
    v
hot graph execution
```

For local single-user mode, no TrustGrant configuration should be required.

---

## 66.6.5 TrustGrant-aware architecture/security analysis

Beyond protecting SyntaxMesh itself, SyntaxMesh can understand TrustGrant usage inside analyzed applications.

Potential graph facts:

```text
handler --REQUIRES_CAPABILITY--> inventory.purchase
capability --AUTHORIZES--> operation
operation --MUTATES_RESOURCE--> inventory
```

This enables queries such as:

```text
Which externally reachable state-changing operations do not
have a declared authorization relationship?

Which handlers can indirectly reach privileged resources?

Which capabilities are defined but unused?

Which service paths bypass the intended authorization layer?
```

SyntaxMesh provides static/system intelligence.

TrustGrant remains the authority protocol.

---

## 66.6.6 stexs-bevy interoperability

`stexs-bevy` is the game-engine-side integration point and should be understood as a first-class analysis target.

Potential Bevy graph extraction:

```text
System
Plugin
Resource
Component
Event
Schedule
State
NetworkMessage
```

Potential relations:

```text
SYSTEM_READS_RESOURCE
SYSTEM_WRITES_RESOURCE
SYSTEM_EMITS_EVENT
SYSTEM_CONSUMES_EVENT
PLUGIN_ADDS_SYSTEM
MESSAGE_SENT_TO_BACKEND
MESSAGE_RECEIVED_FROM_BACKEND
```

Cross-repository graph example:

```text
Bevy PurchaseSystem
      |
      | SENDS
      v
PurchaseItemRequest
      |
      | IMPLEMENTS_CONTRACT
      v
backend protocol
      |
      | HANDLED_BY
      v
InventoryGateway
```

This gives STEXS-specific value without making core SyntaxMesh dependent on Bevy.

Implement as a language/framework analysis adapter/plugin.

---

## 66.6.7 Future closed-source STEXS multiplayer backend interoperability

The multiplayer backend should be a major **internal** dogfooding target for SyntaxMesh.

Because the platform is closed source, its SyntaxMesh adapters/plugins remain private and are not shipped as public first-party integrations.

SyntaxMesh should help answer:

```text
What services are affected by this protocol change?

Which game/client versions depend on this contract?

Which tests must run?

Which backend services require rebuild/redeploy?

Which data-plane services depend on control-plane code?

Which public handlers can mutate StateChronicle-backed resources?

Which operations require which TrustGrant capabilities?

Which Penelope workflows span these services?

Which artifacts are produced/consumed through Shardline?

Which Bevy systems consume a changed message?
```

This should drive real features and benchmarks.

Do not introduce multiplayer-specific assumptions into generic graph-core merely to satisfy STEXS dogfooding.

Use plugin/layer semantics.

The private multiplayer platform should inject its own internal extensions through `syntaxmesh-extension-sdk`, while public SyntaxMesh only ships integrations for STEXS projects that are themselves open source.

---

## 66.6.8 STEXS composition model

The intended high-level composition is:

```text
                           STEXS DEVELOPMENT PLATFORM

                                  SyntaxMesh
                        source/system intelligence
                                     |
          +--------------------------+--------------------------+
          |                          |                          |
          v                          v                          v
      CI / policy               IDE / developer            release impact
          |                          |                          |
          +--------------------------+--------------------------+
                                     |
                +--------------------+--------------------+
                |                    |                    |
                v                    v                    v
             Penelope          StateChronicle         TrustGrant
         durable workflows    verified history        authority
                |
                v
             Shardline
       immutable artifacts/cache

                                     |
                                     v
                         multiplayer backend + stexs-bevy
                           analyzed/dogfooded system
```

This diagram expresses responsibility, not mandatory runtime dependencies.

Actual dependency policy:

```text
Penelope:
  mandatory internal dependency for durable SyntaxMesh workflows

StateChronicle:
  day-one integration; runtime verified history opt-in

Shardline:
  optional artifact/cache/bootstrap integration

TrustGrant:
  optional shared/cloud authorization integration

stexs-bevy:
  analysis target/plugin integration, not SyntaxMesh dependency

multiplayer backend:
  closed-source analysis target and primary STEXS dogfooding environment;
  integrations remain private/internal
```

---

## 66.6.9 STEXS interoperability contracts

Do not wire projects together through private internal structs.

Prefer explicit contracts.

Public first-party integration crates/modules:

```text
syntaxmesh-integration-penelope
syntaxmesh-integration-statechronicle
syntaxmesh-integration-shardline
syntaxmesh-integration-trustgrant
syntaxmesh-framework-bevy
syntaxmesh-framework-stexs-bevy
```

Only integrations targeting open-source STEXS projects belong in the public first-party integration set.

The closed-source STEXS platform and its domains use private extensions outside this repository through the same public extension SDK.

Penelope remains a full-engine reliability dependency as already defined; StateChronicle remains a day-one integration with opt-in runtime verification.

Each optional integration should:

- [ ] depend on public/stable STEXS APIs only
- [ ] have its own conformance/integration tests
- [ ] fail independently without corrupting canonical graph state
- [ ] remain removable from builds through Cargo features where appropriate
- [ ] avoid circular crate dependencies between STEXS projects
- [ ] document version compatibility
- [ ] use typed IDs at boundaries
- [ ] propagate provenance into graph facts
- [ ] avoid hidden network calls

---

## 66.6.10 Cloud API design

Cloud/team deployment should expose the same logical query model as local usage.

Recommended service surfaces:

```text
HTTP API
streaming NDJSON
MCP
optional local IPC
```

Potential routes:

```text
GET  /v1/workspaces/{id}/status
POST /v1/workspaces/{id}/query
GET  /v1/workspaces/{id}/export.ndjson
POST /v1/workspaces/{id}/impact
POST /v1/workspaces/{id}/rules/check
POST /v1/workspaces/{id}/diff
```

Do not expose raw SQL against Turso.

Do not expose the database file.

The application/query layer owns semantics.

For large graph responses, stream NDJSON rather than buffering a monolithic response.

---

## 66.6.11 Git/webhook ingestion

Persistent cloud deployments should support repository events.

Concept:

```text
Git provider webhook
      |
      v
workspace/repository resolver
      |
      v
Penelope indexing workflow
      |
      v
fetch/update repository
      |
      v
incremental graph update
      |
      +--> StateChronicle generation verification if enabled
      +--> DuckDB analytical sync
      +--> Shardline immutable snapshot publication if configured
```

Git-provider-specific webhook adapters should remain outside graph-core.

---

## 66.6.12 Cloud scaling strategy

Scale by isolating independent workspaces.

Preferred:

```text
workspace hash/routing
        |
        +--> worker 1: workspaces A, D
        +--> worker 2: workspaces B, F
        +--> worker 3: workspaces C, E
```

A worker may own multiple small workspaces.

Very large workspaces may receive dedicated workers.

Do not prematurely distribute one graph traversal across a cluster.

First scale:

1. repositories/workspaces horizontally
2. extraction in parallel within a worker
3. analytical datasets through DuckDB/Parquet
4. immutable bootstrap artifacts through Shardline

Only investigate distributed graph execution if real workloads prove one machine insufficient.

---

## 66.6.13 Cloud recovery and relocation

Because the live DB is embedded/local, worker recovery must be explicit.

Possible recovery sources, in order:

```text
1. local persistent volume
2. compatible deterministic SyntaxMesh snapshot from Shardline
3. source repositories + rebuild
```

Penelope ensures incomplete workflows reconcile correctly.

StateChronicle, when enabled, can verify that the restored/rebuilt generation corresponds to accepted history.

DuckDB remains rebuildable analytical state.

This gives a cloud deployment strong recovery semantics without centralizing live graph state in an external DB service.

---

## 66.6.14 Product positioning after cloud support

Do not describe SyntaxMesh as only:

> a local code graph tool

Prefer:

> **A local-first source and system intelligence engine for software repositories and engineering platforms.**

"Local-first" means:

```text
works completely offline
embedded database
no mandatory hosted service
self-hostable everywhere
```

It does not mean:

```text
single-user only
laptop only
one repository only
no CI
no Kubernetes
no team service
no cross-repository analysis
```

---


# 66.7 Open-source and private-integration boundary

SyntaxMesh itself should be an open-source STEXS-Technologies project.

The public project should contain:

```text
SyntaxMesh core/engine
Turso storage backend
SQLite reference backend
DuckDB analytics layer
extension SDK/protocol
language/source analyzers
public APIs
CLI/daemon/MCP/HTTP adapters
first-party integrations for open-source STEXS projects
```

The public project should **not** contain proprietary STEXS platform semantics.

---

## 66.7.1 Official first-party integration policy

Public first-party domain/system integrations are limited to STEXS-Technologies projects that are themselves publicly open source.

Current intended first-party integrations:

```text
Penelope
StateChronicle
TrustGrant
Shardline
stexs-bevy
```

These integrations should be public because:

- their target systems are public
- they demonstrate the extension SDK
- users of those projects benefit directly
- behavior can be reviewed and reproduced
- they provide dogfooding for SyntaxMesh's extension contracts

Do not label arbitrary third-party ecosystem adapters as "first-party" merely to increase integration count.

---

## 66.7.2 Generic capabilities are not vendor integrations

This policy does not prevent SyntaxMesh itself from understanding generic source/configuration standards required for its core mission.

Examples that may reasonably live in the public project:

```text
Rust
Python
TypeScript
SQL
OpenAPI
protobuf
GraphQL
Cargo metadata
generic Git
generic CI metadata
generic OpenTelemetry ingestion contract
```

The distinction is:

```text
generic interoperability primitive
    can belong in SyntaxMesh

vendor/company/domain-specific system integration
    extension
```

---

## 66.7.3 Community and third-party integrations

Users/community projects may publish their own integrations independently.

Examples:

```text
syntaxmesh-temporal
syntaxmesh-kafka
syntaxmesh-company-x
syntaxmesh-unity
syntaxmesh-unreal
```

SyntaxMesh should document how to build these but does not need to own or maintain them.

No official-support implication should arise merely because an integration uses the public SDK.

Maintain a clear distinction between:

```text
core
STEXS first-party
community
private/internal
```

---

## 66.7.4 Closed-source STEXS platform integrations

The future closed-source STEXS multiplayer platform should consume open-source SyntaxMesh through private integration crates/modules.

Example internal-only crates:

```text
syntaxmesh-stexs-platform
syntaxmesh-stexs-matchmaking
syntaxmesh-stexs-inventory
syntaxmesh-stexs-economy
syntaxmesh-stexs-control-plane
syntaxmesh-stexs-deployment
```

Names are illustrative.

These MUST live outside the public SyntaxMesh repository.

They can inject proprietary facts such as:

```text
internal service topology
matchmaking shard ownership
inventory/economy domain relationships
deployment units
private protocol relationships
operational workflows
internal security boundaries
customer/tenant routing
runtime topology
```

without exposing them publicly.

---

## 66.7.5 Private plugins use the same public SDK

Do not create a privileged hidden plugin interface only for STEXS.

The closed platform should consume:

```text
syntaxmesh-extension-sdk
syntaxmesh-runtime-protocol
syntaxmesh-api-model
```

exactly like an external user.

This is deliberate dogfooding.

If the STEXS platform needs a capability that the public extension API cannot express, first ask whether that capability is generally useful.

If yes:

```text
improve public extension API
    |
    v
consume it privately
```

Do not add private hooks into SyntaxMesh core.

This keeps the public engine honest and prevents architectural drift toward internal-only assumptions.

---

## 66.7.6 Internal embedding model

The closed STEXS platform may embed the open-source SyntaxMesh engine directly:

```text
closed STEXS platform process
|
+-- open-source SyntaxMesh engine
|
+-- public first-party extensions
|     +-- Penelope
|     +-- StateChronicle
|     +-- TrustGrant
|     +-- Shardline
|
+-- private STEXS extensions
      +-- matchmaking
      +-- inventory
      +-- economy
      +-- deployment
      +-- proprietary runtime
```

This produces one coherent graph while preserving repository/licensing boundaries.

The private extensions can remain statically linked internal crates if that is the simplest and fastest deployment model.

---

## 66.7.7 Public/private data boundary

The engine must never assume graph facts are safe to export merely because SyntaxMesh itself is open source.

Private integrations may inject:

```text
proprietary service names
internal architecture
security relationships
customer-specific topology
deployment information
runtime observations
```

Therefore export/query authorization must operate on:

```text
workspace
graph layer
namespace
provenance/source
fact visibility
```

where shared/team deployments require it.

A private graph can contain public-source facts and proprietary runtime/system facts simultaneously.

---

## 66.7.8 Namespace ownership

Reserve namespaces.

Examples:

```text
core:*
rust:*
bevy:*
penelope:*
statechronicle:*
trustgrant:*
shardline:*
```

Private STEXS may reserve:

```text
stexs.internal:*
```

External organizations can use reverse-domain or organization-qualified namespaces:

```text
com.acme.payment:*
io.example.platform:*
```

Prevent two extensions from silently claiming the same namespace.

---

## 66.7.9 Licensing objective

Use a permissive license compatible with embedding SyntaxMesh in closed-source software.

The existing planned:

```text
MIT OR Apache-2.0
```

model fits this objective well.

The open-source engine should remain independently useful.

Closed-source STEXS plugins and platform code do not need to be published merely because they link against or embed SyntaxMesh under a permissive license.

Review exact third-party dependency licenses before release.

---

## 66.7.10 No open-core degradation

Do not intentionally cripple the public SyntaxMesh engine to make the closed STEXS platform useful.

Public SyntaxMesh should contain the complete general-purpose intelligence substrate:

```text
source indexing
graph execution
runtime observation protocol
extension SDK
workspace model
CI/impact analysis
architecture policy
history/analytics
APIs
```

The private platform advantage comes from:

```text
proprietary domain knowledge
private adapters
platform-specific workflows
internal runtime observations
hosted product UX/operations
```

not from withholding generic engine functionality.

This keeps SyntaxMesh credible as an open-source infrastructure project.

---

## 66.7.11 Repository organization

Suggested public repository structure:

```text
crates/
    syntaxmesh-core/
    syntaxmesh-engine/
    syntaxmesh-extension-sdk/
    syntaxmesh-extension-protocol/
    syntaxmesh-runtime-protocol/
    ...

integrations/
    penelope/
    statechronicle/
    trustgrant/
    shardline/
    bevy/
    stexs-bevy/

languages/
    rust/
    python/
    typescript/
    ...
```

Closed-source STEXS repositories/workspaces depend on released/git versions of the public crates and keep private integrations internally.

No private plugin source is vendored into the public repo.

---


# 67. Naming

## Recommended working name: SyntaxMesh

Why:

- short
- understandable
- strongly associated with source structure
- "mesh" communicates many connected facts better than a simple tree
- not tied to one AI vendor
- not tied to SQLite/DuckDB
- can describe both the engine and the graph
- broad web search on 2026-09-22 did not surface an established software project using the exact `SyntaxMesh` name, although package/domain/trademark checks must still be performed before release

Potential CLI/crate naming:

```text
syntaxmesh
syntaxmesh-core
syntaxmesh-store
syntaxmesh-mcp
syntaxmeshd
```

Tagline:

> Incremental source intelligence for humans and coding agents.

Alternative:

> A local-first code intelligence graph engine.

Other names considered but less attractive because they already collide with existing projects/products or are too generic:

- Graphline — already used
- SourceKnot — already used
- SourceMesh — already used
- ContextMesh — already heavily used
- RepoMesh — already used
- CodeAtlas — heavily used
- CodeTrellis — used
- GraphLoom — used
- Knotwork — used
- Threadmark — used
- RepoWeave — used

Before publication:

- [ ] search GitHub exact name
- [ ] search crates.io exact name
- [ ] search package managers
- [ ] search domains
- [ ] search basic EU/US trademark databases if branding becomes serious
- [ ] reserve GitHub repo
- [ ] reserve crates.io package when API is ready
- [ ] reserve documentation domain if desired

---

# 68. Repository ownership: STEXS-Technologies vs personal GitHub

## Recommendation: publish publicly under STEXS-Technologies

For this project, an organization repository is the better long-term location.

The current STEXS GitHub bio is narrower than the projects already hosted there. It currently emphasizes multiplayer backends, game-engine integration, secure devices, real-time control, and autonomous platforms, while the actual public repositories now also include content-addressed storage, generic saga orchestration, verifiable resource state, and authority/delegation infrastructure.

SyntaxMesh fits the **actual emerging identity** of STEXS very well: reusable, correctness-oriented Rust infrastructure.

The organization positioning should therefore be broadened rather than treating SyntaxMesh as an outlier.

Suggested organization description:

> Building open-source Rust infrastructure for multiplayer game backends, storage, orchestration, verifiable state, authorization, and real-time systems.

Shorter option:

> Building Rust infrastructure for multiplayer game backends and the reliable systems behind them.

This also describes Shardline, Penelope, StateChronicle, TrustGrant, and future infrastructure projects more accurately than the current domain-specific bio.

Reasons:

### 68.1 It is infrastructure, not a throwaway experiment

SyntaxMesh is a reusable developer platform/library/engine.

That belongs naturally under an engineering organization rather than being framed as a personal toy repository.

### 68.2 Organizational continuity

An org makes it easier later to have:

- multiple maintainers
- teams
- bot/service accounts
- protected release workflows
- package publishing
- security policies
- documentation sites
- governance
- sponsorship
- CI secrets
- future commercial integrations

### 68.3 Portfolio effect

A serious source-intelligence engine strengthens the STEXS-Technologies identity as a producer of infrastructure components.

The project can still clearly list the original author/maintainer.

### 68.4 Avoid migration later

If the project becomes successful, moving from a personal account to the org later creates unnecessary branding, permission, automation, package, and URL churn.

Start in the intended permanent home.

### 68.5 When personal would make sense

Use the personal account only if:

- this is intentionally a short-lived spike
- the repo is private and disposable
- you do not yet want it associated with STEXS
- you are testing whether the concept deserves a real project

A reasonable workflow is:

```text
private prototype anywhere
        |
architecture proven
        |
public repository created directly under STEXS-Technologies
```

Do not publicly launch it on the personal account and then migrate it a week later.

---

# 69. README positioning

Do not position it as:

> Graphify rewritten in Rust.

That makes the project sound derivative and invites feature-by-feature comparison.

Position it as:

> SyntaxMesh is a local-first incremental source-intelligence database and graph engine for software repositories.

Then explain the problems:

- codebases are too large to reread
- grep lacks architecture
- serialized graph snapshots go stale
- coding agents need minimal evidence, not giant context dumps
- repository structure changes continuously

Key differentiators:

```text
Rust-native
transactional
incremental
typed
Turso-backed
DuckDB analytics
Git-aware
provenance-first
tooling-neutral
local-first
```

Agents are an important integration target, not the product boundary.

Primary consumer classes:

```text
developers
CLI tools
IDEs/editors
CI systems
code review systems
architecture tooling
security tooling
documentation tooling
repository analytics
refactoring tools
developer portals
coding agents
```

MCP belongs alongside CLI/HTTP/IDE/CI adapters, not above them.

README should also make clear:

> SyntaxMesh is extensible by design. Its public extension SDK lets teams model their own infrastructure, runtime semantics, and proprietary systems without forking the engine.

Public STEXS integrations demonstrate this mechanism; the closed-source STEXS platform uses the same extension boundary internally.


---


# 69.5 STEXS composition: Penelope and StateChronicle

SyntaxMesh should deliberately reuse STEXS infrastructure where the semantics fit, but should not turn those projects into mandatory dependencies of the hot read/query path.

The right boundary is:

```text
                       SyntaxMesh
                           |
          +----------------+----------------+
          |                                 |
          v                                 v
  hot source/graph path               durable operations
  Turso + graph engine                     |
          |                                +----------------------+
          |                                |                      |
          v                                v                      v
      queries                         Penelope            StateChronicle
                                  workflow reliability    verified history
```

Penelope and StateChronicle solve different failure classes.

---

## Penelope integration

Penelope is a deterministic saga/process orchestration library over an append-only outcome log. It already provides crash-safe replay, deduplication, stale-result rejection, bounded retries, compensation, timers, reconciliation, and backend-neutral durable ports.

SyntaxMesh has several long-running operations that are naturally sagas rather than ordinary function calls.

### Operations that should use Penelope

#### Full repository indexing

```text
discover repository
    ->
scan files
    ->
hash/classify
    ->
parse
    ->
extract
    ->
resolve
    ->
commit graph generation
    ->
build graph projection
    ->
sync DuckDB analytics
    ->
publish generation
```

If the process dies after the Turso commit but before the graph projection or DuckDB synchronization finishes, blindly rerunning every stage is wasteful and can create ambiguous state.

Penelope can record which effects definitely completed.

#### Incremental re-index generation

Treat each index generation as a scoped workflow:

```text
GenerationId
RepositoryId
BaseGraphGeneration
TargetWorkingTreeState
```

Penelope gives typed scopes that prevent late results from an old generation being applied to a newer repository state.

This is particularly useful when files change again while parsing is still underway.

Example:

```text
g104 starts parsing src/auth.rs

file changes again

g105 starts

g104 parser returns late
```

Without strict generation identity, stale extraction results can pollute the current graph.

Penelope's typed identity/stale-result model is directly useful here.

#### Git snapshot import/export

Snapshot creation can involve:

- freezing a graph generation
- materializing records
- compression
- writing checksum
- atomic rename
- registering metadata
- analytics export

This should survive interruption without publishing a partial snapshot.

#### Semantic enrichment

LLM/local-model enrichment is an external effect with exactly the uncertainty Penelope is designed to handle:

```text
request dispatched
network/process failure
did the model complete?
was result persisted?
retry safe?
```

Use Penelope for:

- retries
- timeouts
- deduplication
- result correlation
- reconciliation
- manual review for corrupted/ambiguous results

#### Large migrations

Examples:

- schema migration requiring graph rebuild
- re-running a new resolver version
- rebuilding all provenance
- adding a new language extractor version
- converting historical analytics format

These should be resumable workflows rather than giant one-shot commands.

#### CI/PR analysis pipelines

For a CI job:

```text
checkout
index base
index head
graph diff
rule evaluation
impact analysis
report generation
```

Penelope can make the process deterministic and resumable when used in persistent CI services.

### Penelope should NOT own

Do not route these through Penelope:

- symbol lookup
- one-hop neighbors
- path queries
- BFS
- FTS search
- graph projection reads
- ordinary single-file in-process parsing
- every Turso transaction

The hot graph path should stay direct.

### Suggested integration crate

```text
syntaxmesh-penelope
```

Keep it optional at first if practical.

Potential workflow definitions:

```text
IndexRepositorySaga
IncrementalGenerationSaga
SnapshotExportSaga
AnalyticsSyncSaga
SemanticEnrichmentSaga
MigrationSaga
```

The core application service can use the integration internally without exposing Penelope concepts in the public graph API.

---

## StateChronicle integration

StateChronicle provides append-only signed history, deterministic replay, cryptographic state roots, optimistic concurrency, atomic batch planning, and portable verification proofs.

That maps extremely well to **graph generation history**, but not to individual graph query execution.

### Key idea

Treat each published SyntaxMesh graph generation as a verifiable resource state transition.

Example:

```text
RepositoryGraph(repo_id)
generation 5819
    ->
generation 5820
```

The transition has a deterministic delta:

```text
files added/changed/deleted
nodes added/changed/deleted
edges added/changed/deleted
provenance changes
rule results
extractor versions
base git state
```

StateChronicle can provide an append-only record proving the sequence of published graph states.

### What this improves

#### Tamper-evident graph history

A repository intelligence database may be used for:

- security analysis
- compliance
- CI policy
- architecture enforcement
- code review
- audit evidence

For those cases, being able to prove:

```text
"This graph generation is exactly the state produced by these recorded transitions."
```

is valuable.

#### Deterministic graph-generation roots

After canonical graph commit:

```text
GraphStateRoot = hash(canonical graph state)
```

StateChronicle can bind transitions into a signed/verifiable chain.

This is separate from Turso durability.

Turso answers:

> What state is stored now?

StateChronicle answers:

> What sequence of accepted transitions produced it, and can that history be verified?

#### Audit trail for architecture decisions

Example event types:

```text
RepositoryRegistered
FileObserved
GraphGenerationPrepared
GraphGenerationPublished
GraphGenerationRejected
ArchitectureViolationIntroduced
ArchitectureViolationResolved
SnapshotPublished
ExtractorVersionChanged
ResolverVersionChanged
```

Do not log every internal parser event by default.

Record semantically meaningful committed transitions.

#### Rebuild verification

After a rebuild:

```text
source state
   ->
deterministic extraction
   ->
new graph state root
```

Compare the root against the recorded expected generation.

This detects unexpected nondeterminism or corruption.

#### Portable proof artifacts

Future use:

```text
syntaxmesh prove generation 5820
syntaxmesh verify snapshot.smx.zst
```

A CI artifact could include:

- Git commit
- graph-generation ID
- graph state root
- signed commit proof
- extractor/resolver version set
- architecture rule result root

This makes SyntaxMesh useful in supply-chain/compliance contexts.

#### Historical integrity

DuckDB is excellent for analytics but should not be treated as the integrity source.

StateChronicle can certify the sequence of canonical graph generations, while DuckDB stores analytical projections of that history.

```text
Turso
  live canonical state

StateChronicle
  verified transition history

DuckDB
  analytical/history projection
```

That is a clean three-layer split.

### StateChronicle should NOT become the graph database

Do not store millions of graph edges as individual StateChronicle assets/events merely because the library exists.

That would be the wrong abstraction and would add large overhead.

Use StateChronicle at the **graph-generation boundary**.

One StateChronicle transition can refer to a compact graph delta/root rather than representing each internal node and edge as ledger resources.

### Suggested integration crate

```text
syntaxmesh-statechronicle
```

Responsibilities:

- map published graph generations to ledger transitions
- calculate canonical graph state roots
- record graph metadata/provenance roots
- verify replay/rebuild outcomes
- expose proof/verification commands

Keep ordinary local development able to run without signed-history mode if the dependency is considered too heavy for the minimal build.

---

## Penelope + StateChronicle together

The strongest composition is:

```text
                Penelope
         orchestrates generation
                  |
                  v
      parse / resolve / build delta
                  |
                  v
          Turso transaction
                  |
            durable success
                  |
                  v
          StateChronicle
       records/verifies commit
                  |
                  v
        generation published
                  |
           +------+------+
           |             |
           v             v
     graph projection   DuckDB
```

Important rule:

> A SyntaxMesh generation is not considered published until its canonical Turso mutation is durable. When verified-history mode is enabled, the corresponding StateChronicle publication record must also be committed before the generation is exposed as verified/published.

Penelope can coordinate the process.

StateChronicle can verify the accepted state transition.

### Failure example

```text
1. parsing finished
2. Turso graph transaction committed
3. process crashes before StateChronicle record
```

Penelope replays the workflow.

It must **not** repeat the graph mutation blindly.

It checks/correlates the durable result, completes the StateChronicle publication step, and advances.

This is exactly the kind of ambiguous external-result boundary Penelope is designed for.

### Another failure example

```text
1. StateChronicle publication committed
2. DuckDB analytics sync crashes
```

The canonical graph is still valid and published.

Penelope resumes only the derived analytics sync.

DuckDB never becomes a correctness dependency.

---

## Potential graph-generation state machine

```text
Detected
    |
    v
Scanning
    |
    v
Extracting
    |
    v
Resolving
    |
    v
Prepared
    |
    v
CanonicalCommitted
    |
    v
Verified
    |
    v
Published
    |
    +------> AnalyticsSynced
    |
    +------> ProjectionReady
```

Do not expose half-complete states as current.

Readers should continue using the previous published generation until the new one reaches `Published`.

---

## Resulting correctness model

SyntaxMesh can eventually claim a stronger model than ordinary code graph tools:

```text
source facts
   |
deterministic extraction
   |
typed graph delta
   |
atomic Turso mutation
   |
verified StateChronicle transition
   |
Penelope-recoverable publication workflow
   |
immutable graph generation
```

This is especially relevant to STEXS because it reuses the same reliability primitives intended for multiplayer/backend state systems in developer infrastructure.

---

## Do not force integration merely for branding

Use each library only where its semantics are genuinely stronger than a bespoke implementation.

Penelope is justified for long-lived, retryable, externally effected workflows.

StateChronicle is justified for optional verifiable graph-generation history.

Do not use either in simple graph reads or tight parsing loops.

---

# 69.6 STEXS organization positioning

STEXS is still primarily moving toward multiplayer game-backend infrastructure.

The organization description should **broaden**, not abandon that identity.

Current public repositories already cover multiplayer/game integration plus reusable infrastructure such as storage, workflow orchestration, state, and authority.

A better organization description should preserve multiplayer as the lead while making the infrastructure direction explicit.

Recommended:

> **Building open-source Rust infrastructure for multiplayer game backends, storage, orchestration, verifiable state, authorization, and real-time systems.**

Shorter:

> **Open-source Rust infrastructure for multiplayer backends and reliable real-time systems.**

More platform-oriented:

> **Building Rust infrastructure for multiplayer game backends and the reliable systems behind them.**

The third version is probably the strongest umbrella.

It keeps the multiplayer destination visible while naturally allowing:

- Shardline
- Penelope
- StateChronicle
- TrustGrant
- SyntaxMesh
- future networking/runtime/backend components

SyntaxMesh then fits as infrastructure produced by the same engineering organization, even though its direct use case is broader than games.


# 70. Implementation roadmap

> **Status crosswalk (2026-09-29):** The unchecked phase lists below are the
> original product roadmap, not a live checklist; some entries are implemented
> while others remain open. Use [`V0_ARCHITECTURE_PLAN.md`](V0_ARCHITECTURE_PLAN.md)
> as the implementation ledger and the linked ADRs/tests as evidence. Current
> phase-level status is: Phase 0 substantially implemented; Phase 1 implemented
> for the WAL/reference slice but capability detection, safe concurrent-write
> support, and full-text search remain open; Phase 2 implemented except for
> bounded parallel scanning and broader binary/generated-file policy; Phase 3
> implemented as a starter Rust pack; Phase 4 partial (conservative resolution,
> diagnostics, and invalidation exist; lexical/compiler-aware resolution does
> not); Phase 5 partial (generation-pinned search/traversal/impact exist; SCC,
> cycle analysis, and concurrent reads during publication remain open); Phase 6
> not started; Phase 7 partial (read-only MCP exists, token-budgeted context and
> broader host coverage remain open); Phase 8 has starter packs for the in-scope
> formats, with semantic completeness, monorepo and cross-language coverage
> still open; Phase 9 not started; Phase 10.4 partial (embedded engine, extension
> SDK, runtime observations, Penelope, and StateChronicle integration exist;
> out-of-process extension protocol and daemon equivalence remain open). Later
> workspace/cloud, Git-history, analytics, architecture-rule, visualization,
> and hardening phases remain future work. This summary is deliberately
> conservative; see the architecture plan for exact implemented slices and
> acceptance evidence.

## Phase 0 — architecture lock

- [ ] Create repository under STEXS-Technologies.
- [ ] Add Penelope as a mandatory internal workspace/dependency from day one.
- [ ] Define Penelope workflow identities and generation correlation before indexing orchestration is implemented.
- [ ] Add StateChronicle integration surface from day one.
- [ ] Define graph-generation manifest/root format required for later/optional StateChronicle verification.
- [ ] Keep StateChronicle runtime verification opt-in initially.
- [ ] Define the public versioned NDJSON graph DTO/export schema before visualization consumers appear.
- [ ] Define the extension API/versioning model before proprietary/internal integrations are implemented.
- [ ] Define namespace ownership/registration rules.
- [ ] Confirm permissive licensing supports private embedding/integration.
- [ ] Reserve working name.
- [ ] Choose Apache-2.0 OR MIT/Apache-2.0 dual license.
- [ ] Add `ARCHITECTURE.md`.
- [ ] Add ADR framework.
- [ ] Define core invariants.
- [ ] Define threat model.
- [ ] Define benchmark methodology before performance work.
- [ ] Create Cargo workspace.
- [ ] Add CI: fmt, clippy, test, deny/audit, MSRV if desired.

Exit criteria:

- storage boundary defined
- core model defined
- no code depends on a specific DB except backend crate

---

## Phase 1 — canonical Turso store

- [ ] Implement repository/file/node/edge/provenance schema.
- [ ] Implement migrations.
- [ ] Implement `GraphStore`.
- [ ] Implement Turso backend first.
- [ ] Turso WAL embedded mode.
- [ ] transaction batching.
- [ ] FTS/symbol-search capability.
- [ ] capability detection.
- [ ] experimental concurrent/MVCC feature gate.
- [ ] transaction conflict retry framework.
- [ ] storage conformance suite.
- [ ] DB integrity checks.
- [ ] `status` command.

Exit criteria:

- typed nodes/edges can be inserted, queried, updated, removed atomically
- restart preserves state
- migration tests pass

---

## Phase 2 — scanner and content identity

- [ ] filesystem scanner
- [ ] ignore support
- [ ] Git ignore support
- [ ] path normalization
- [ ] BLAKE3 hashing
- [ ] file-state table
- [ ] changed/new/deleted detection
- [ ] bounded parallel scanning
- [ ] binary/generated-file policy

Exit criteria:

- unchanged repository scan performs zero parser work
- file deletion is recognized correctly
- path handling works cross-platform

---

## Phase 3 — language SDK + Rust extractor

- [x] Rust syntax parsing through the Rust-native `syn` crate
- [x] language plugin trait
- [x] extraction event types
- [x] Rust language pack
- [x] definitions
- [x] typed Rust `use`-tree import facts (named and glob), with resolution and public re-export semantics still open ([ADR-0077](adr/0077-rust-use-source-facts.md))
- [x] source-backed call references
- [x] modules
- [x] functions
- [x] methods
- [x] structs/enums/traits
- [x] tests
- [x] benchmark fixtures

Exit criteria:

- real Rust repositories produce deterministic typed facts

---

## Phase 4 — resolution engine

- [ ] lexical scope
- [ ] module graph
- [ ] imports/aliases
- [ ] qualified names
- [ ] unresolved-reference persistence
- [ ] resolver diagnostics
- [ ] edge provenance
- [ ] dependency-aware re-resolution

Exit criteria:

- cross-file calls/import relationships work on representative projects
- unresolved references are explicit rather than silently dropped

---

## Phase 5 — graph execution

- [ ] adjacency projection
- [ ] reverse adjacency
- [ ] BFS/DFS
- [ ] path
- [ ] callers/callees
- [ ] impact
- [ ] SCC/cycles
- [ ] projection generation ID
- [ ] concurrent reads during updates

Exit criteria:

- graph query latency independent of full JSON serialization
- MCP can query while indexing occurs

---

## Phase 6 — daemon/watch mode

- [ ] `syntaxmeshd`
- [ ] filesystem events
- [ ] debounce
- [ ] single writer coordination
- [ ] IPC
- [ ] health/status
- [ ] graceful shutdown
- [ ] crash recovery
- [ ] stale PID/socket cleanup

Exit criteria:

- save file -> graph updates automatically
- multiple CLI clients cannot corrupt state

---

## Phase 7 — MCP and agent context

- [ ] MCP server
- [ ] search
- [ ] node
- [ ] neighbors
- [ ] path
- [ ] impact
- [ ] explain
- [ ] context
- [ ] token budgeter
- [ ] source evidence
- [ ] graph generation metadata

Exit criteria:

- coding agent can answer architectural questions without whole-repo rereads

---

## Phase 8 — TypeScript / JavaScript / Python

All language packs are Rust implementations running inside SyntaxMesh. Rust,
TypeScript/JavaScript, Python, and Bash name source formats to analyze; they do
not add or invoke those languages' runtimes. Markdown and plain-text documents
are also indexed. Other source languages and runtime implementations are out of
scope unless the product scope is explicitly revised.

- [x] TypeScript language pack
- [x] JavaScript language pack
- [x] Python language pack
- [x] Rust-native Bash syntax extraction for script/function/static-command facts; see [ADR-0073](adr/0073-bash-and-documentation-source-facts.md).
- [x] Node and Python module-resolution tests, including one configured mixed-language project
- [x] Basic monorepo-resolution test: the Rust CLI resolves TypeScript and Python imports across separate packages under one repository root, while Node/Python profiles leave Rust imports untouched (`crates/syntaxmesh-cli/tests/monorepo_resolution.rs`). This does not establish broad monorepo compatibility or cross-language edges.
- [ ] cross-language edge model where contracts allow

Exit criteria:

- realistic polyglot repository works

---

## Phase 9 — DuckDB analytics

- [ ] define analytical schema
- [ ] `syntaxmesh-analytics`
- [ ] DuckDB integration
- [ ] Arrow/Parquet export
- [ ] incremental analytical sync
- [ ] graph metrics history
- [ ] ingestion/query performance dashboards
- [ ] cross-snapshot SQL

Exit criteria:

- analytics can be rebuilt from canonical/history data
- DuckDB failure does not damage live graph

---



## Phase 10.4 — runtime-agnostic engine and probes

- [ ] extract all daemon-independent application semantics into `syntaxmesh-engine`
- [ ] ensure `syntaxmeshd` is a host/adapter only
- [ ] define stable embedded Rust API
- [ ] create `syntaxmesh-runtime-protocol`
- [ ] create `syntaxmesh-extension-sdk`
- [ ] create `syntaxmesh-extension-protocol`
- [ ] implement extension manifests and namespace registration
- [ ] implement in-process Rust extension registration
- [ ] define out-of-process fact/observation protocol
- [ ] build extension conformance testkit
- [ ] create runtime observation DTOs
- [ ] define provenance/trust classes for runtime facts
- [ ] build in-process observation adapter
- [ ] build streaming/batch observation adapter
- [ ] aggregation/dedup before permanent graph insertion
- [ ] production/development collection policies
- [ ] keep probes Penelope/Turso/DuckDB-free
- [ ] build Penelope runtime-observation adapter
- [ ] build StateChronicle committed-transition observation adapter
- [ ] build TrustGrant authorization observation adapter
- [ ] update Bevy/STEXS-Bevy integration to emit runtime observations
- [ ] verify daemon and embedded host produce equivalent query results
- [ ] benchmark probe overhead and enforce strict budgets

Exit criteria:

- SyntaxMesh can be embedded directly into a trusted Rust service without running a separate daemon
- a lightweight game/SDK probe can emit useful structural observations without linking the full engine
- static and runtime facts coexist without overwriting each other
- authoritative and untrusted observations remain distinguishable
- Penelope remains mandatory for full engine workflows but absent from thin probes

---

## Phase 10.5 — workspace and cloud service mode

- [ ] introduce `WorkspaceId`
- [ ] support multiple repositories per workspace
- [ ] define workspace ownership/lifecycle
- [ ] persistent daemon service mode
- [ ] HTTP API over application/query layer
- [ ] streaming NDJSON responses
- [ ] health/readiness/status endpoints
- [ ] workspace routing abstraction
- [ ] Kubernetes/container deployment docs
- [ ] persistent-volume recovery tests
- [ ] explicit prohibition/documentation for shared live DB access
- [ ] webhook-triggered indexing adapter boundary
- [ ] cloud worker relocation/bootstrap tests

Exit criteria:

- one SyntaxMesh runtime can serve a team/workspace without any hosted database dependency
- workspace can recover from snapshot/source after worker replacement
- local CLI and cloud HTTP produce equivalent logical query results

---

## Phase 10.6 — public STEXS integrations and private-platform dogfooding

- [ ] Shardline immutable snapshot/bootstrap integration
- [ ] Shardline artifact manifest/version contract
- [ ] TrustGrant workspace/query authorization adapter
- [ ] TrustGrant capability namespace for SyntaxMesh operations
- [ ] Bevy/stexs-bevy framework graph extractor
- [ ] StateChronicle resource/workflow relationship extraction where appropriate
- [ ] Penelope workflow relationship extraction where appropriate
- [ ] public first-party integrations only for open-source STEXS projects
- [ ] private closed-source STEXS multiplayer backend dogfood fixture/workspace in internal repositories
- [ ] verify private platform plugins require no private SyntaxMesh-core hooks
- [ ] cross-repository protocol/message analysis
- [ ] selective CI planner
- [ ] deployment-impact planner
- [ ] architecture rules for control-plane/data-plane separation

Exit criteria:

- SyntaxMesh can analyze a representative STEXS multiplayer workspace across client/integration/backend boundaries through private internal extensions
- public first-party integrations are limited to open-source STEXS projects
- optional STEXS integrations remain outside graph-core and fail independently
- private STEXS integrations consume only public extension contracts
- no project duplicates responsibility owned by another STEXS component

---

## Phase 10 — Git snapshots and diff

- [ ] commit identity
- [ ] working-tree identity
- [ ] graph snapshot metadata
- [ ] structural sharing strategy
- [ ] graph diff
- [ ] rename/move detection
- [ ] branch comparison
- [ ] worktree awareness

Exit criteria:

- `syntaxmesh diff HEAD~1 HEAD` reports semantic graph changes

---

## Phase 11 — storage concurrency hardening

Turso already exists from Phase 1. This phase focuses on advancing from the safe embedded WAL profile toward genuine concurrent-write execution when upstream capabilities permit it.

- [ ] MVCC capability revalidation
- [ ] concurrent-write conformance suite
- [ ] realistic concurrent-ingestion benchmark
- [ ] conflict-rate telemetry
- [ ] retry/backoff tuning
- [ ] index/FTS verification under concurrent mode
- [ ] crash and recovery tests
- [ ] hot-row elimination pass
- [ ] transaction partitioning optimization
- [ ] compare embedded WAL vs embedded MVCC
- [ ] keep SQLite differential/reference backend healthy

Decision gate:

Enable local MVCC by default only if:

- required indexed workloads are supported
- correctness suite passes
- crash/recovery behavior is acceptable
- query semantics are reliable
- real SyntaxMesh workloads materially benefit

---

## Phase 12 — architecture rules

- [ ] TOML rule format
- [ ] rule compiler
- [ ] fast enforcement
- [ ] CI output
- [ ] SARIF output
- [ ] MCP rule query
- [ ] graph diff violations

---

## Phase 13 — docs/schema/infra

- [x] Rust-native Markdown/ADR structural parser plus verbatim, source-spanned prose/code/table chunks attached to the nearest section/document for search and context; inferred decision/code relationships remain open ([ADRs 0073](adr/0073-bash-and-documentation-source-facts.md) and [0111](adr/0111-source-grounded-document-chunks.md)).
- [x] Conservative local Markdown links to indexed document roots and supported source files; source links resolve to exact indexed file paths only, not anchors or symbols ([ADRs 0074](adr/0074-resolve-local-markdown-document-links.md) and [0110](adr/0110-resolve-local-markdown-source-file-links.md)).
- [ ] SQL schema parser
- [ ] OpenAPI
- [ ] GraphQL
- [ ] Docker/Compose
- [ ] Terraform
- [ ] Kubernetes
- [ ] CI config

Only add formats with a clear typed graph model.

---

## Phase 14 — optional semantic enrichment

- [ ] semantic backend trait
- [ ] Ollama/local provider
- [ ] configurable remote providers
- [ ] model-aware cache
- [ ] prompt fingerprints
- [ ] provenance
- [ ] cost accounting
- [ ] resume/checkpoint
- [ ] redaction

---

## Phase 15 — visualization

- [ ] local UI
- [ ] subsystem-level graph
- [ ] path explorer
- [ ] impact explorer
- [ ] diff explorer
- [ ] provenance inspector
- [ ] rule violations
- [ ] scale tests

---

## Phase 16 — hardening

- [ ] fuzzing
- [ ] corrupt DB recovery
- [ ] huge repository tests
- [ ] malicious source tests
- [ ] Windows filesystem edge cases
- [ ] symlink cycles
- [ ] network filesystem behavior
- [ ] resource limits
- [ ] performance regression CI
- [ ] snapshot compatibility CI
- [ ] migration-from-old-version CI

---

# 71. What NOT to do initially

Avoid these until the foundation proves itself:

- [ ] Do not implement 40 languages immediately.
- [ ] Do not build a Neo4j-compatible server.
- [ ] Do not clone all of Cypher.
- [ ] Do not require embeddings.
- [ ] Do not require an LLM.
- [ ] Do not use DuckDB as the live mutation store.
- [ ] Do not make Turso the only backend while its required local behavior remains immature.
- [ ] Do not make Turso Cloud or any hosted SyntaxMesh service a requirement.
- [ ] Do not share one live embedded Turso DB across multiple pods/processes through network storage.
- [ ] Do not put the full SyntaxMesh engine in latency-sensitive multiplayer/gameplay request paths.
- [ ] Do not require the full SyntaxMesh engine inside game clients or SDKs; use thin probes when runtime observation is desired.
- [ ] Do not turn SyntaxMesh into a raw telemetry/time-series warehouse.
- [ ] Do not treat client observations as authoritative state truth.
- [ ] Do not put SyntaxMesh in latency-sensitive multiplayer/gameplay request paths.
- [ ] Do not turn SyntaxMesh into a deployment orchestrator; it produces impact facts/plans.
- [ ] Do not duplicate Shardline, Penelope, StateChronicle, or TrustGrant responsibilities.
- [ ] Do not hard-code closed-source STEXS platform semantics into SyntaxMesh core.
- [ ] Do not publish proprietary STEXS platform plugins in the public SyntaxMesh repository.
- [ ] Do not create a private/privileged extension API available only to STEXS.
- [ ] Do not turn the public repository into an official-integration catalog for arbitrary third-party products.
- [ ] Do not cripple the open-source engine to reserve generic capabilities for the closed-source platform.
- [ ] Do not build a distributed cluster.
- [ ] Do not add Turso Cloud or hosted-database coupling; keep the canonical store embedded/local.
- [ ] Do not build or bundle a graph visualization frontend into SyntaxMesh core.
- [ ] Do not render entire million-node graphs in a browser.
- [ ] Do not generate or persist visualization layout coordinates in the canonical graph.
- [ ] Do provide stable versioned NDJSON graph and subgraph exports for external visualizers.
- [ ] Do not store all graph state as JSON.
- [ ] Do not tie graph identity to line numbers.
- [ ] Do not treat numeric confidence as a substitute for provenance.
- [ ] Do not make Graphify feature parity the release criterion.

---

# 72. v0.1 definition

A good `0.1.0` is already useful if it supports:

```text
Rust
TypeScript
Python

Turso canonical store
Penelope-backed durable indexing workflows
StateChronicle verified-history support (opt-in)
incremental indexing
stable IDs
provenance
versioned NDJSON/subgraph export
symbol search
call/import graph
path queries
impact queries
MCP
token-budgeted context
DuckDB analytical export
Git HEAD/working-tree awareness
```

That would be a coherent product.

Do not wait for PDFs/video/image extraction before publishing.

---

# 73. v1.0 definition

Potential 1.0 bar:

- stable graph schema contract
- stable snapshot format
- stable plugin API
- high-quality Rust implementations for the four in-scope source-language
  families (Rust, TypeScript/JavaScript, Python, Bash) plus documentation
  extraction; do not add source languages without explicit scope revision
- robust Git/worktree support
- Turso production backend
- safe embedded WAL mode
- concurrent/MVCC mode production-enabled if upstream capability is sufficient
- SQLite compatibility/reference backend
- DuckDB historical analytics
- architecture rules
- MCP stable interface
- reproducible benchmarks
- migration guarantees
- cross-platform binaries
- stable embeddable engine API
- stable extension SDK and extension manifest format
- documented in-process and out-of-process extension paths
- namespace/provenance rules for proprietary and community integrations
- runtime observation protocol
- thin SDK/game probe model
- static/runtime evidence separation
- StateChronicle/Penelope/TrustGrant runtime fact adapters
- self-hosted workspace/service mode with no hosted DB requirement
- multi-repository workspace support
- streaming NDJSON HTTP API
- cloud/Kubernetes deployment guidance
- STEXS interoperability adapters for Shardline/TrustGrant/stexs-bevy where mature
- STEXS multiplayer backend dogfooding benchmark
- corruption/crash recovery
- security/threat model docs
- large-monorepo validation

---

# 74. Concrete first implementation order

If starting tomorrow:

```text
1. syntaxmesh-core
2. syntaxmesh-api-model
3. syntaxmesh-engine boundary
4. syntaxmesh-extension-sdk + extension manifest/namespace model
5. syntaxmesh-runtime-protocol + external extension protocol
6. syntaxmesh-store trait
6. Turso schema + migrations
7. storage conformance + SQLite reference backend
8. scanner + BLAKE3
6. Rust Tree-sitter extractor
7. reference model
8. resolver
9. transactional incremental update
10. adjacency projection
11. search/path/impact CLI
12. daemon/watch
13. HTTP/API + IDE-facing query services
14. MCP
15. context budgeter
16. TypeScript/Python
17. DuckDB analytical export
18. Git snapshots/diff
19. Turso concurrent-write hardening
```

This order validates the architecture before broadening scope.

---

# 75. Research notes / current upstream reality

These findings motivated the design.

## Graphify

Graphify currently describes a Python pipeline whose stages exchange plain Python dictionaries and NetworkX graphs.

Its canonical user-facing graph artifact is `graph.json` in NetworkX node-link format.

Its documentation describes parallel Python extraction and content-hash caching.

Current public issues/discussions have included:

- MCP state remaining stale after `graph.json` is updated until reload/restart
- races when rapid commits trigger concurrent rewrites of `graph.json`
- monorepo graphs exceeding GitHub's 100 MB per-file limit
- incremental checkpoint/cache bugs
- missing model/backend identity in graph/cache provenance

Those are not arguments that Graphify is poorly engineered; many are natural consequences of a portable file-oriented architecture growing into an always-on system.

SyntaxMesh should choose a different foundation from the start.

Sources:

- https://github.com/Graphify-Labs/graphify
- https://github.com/Graphify-Labs/graphify/blob/v8/ARCHITECTURE.md
- https://github.com/Graphify-Labs/graphify/blob/v8/docs/how-it-works.md
- https://github.com/Graphify-Labs/graphify/blob/v8/BENCHMARKS.md
- https://github.com/Graphify-Labs/graphify/issues/874
- https://github.com/Graphify-Labs/graphify/issues/1037
- https://github.com/Graphify-Labs/graphify/issues/1708
- https://github.com/Graphify-Labs/graphify/issues/1892
- https://github.com/Graphify-Labs/graphify/issues/2077

## SQLite / libSQL / Turso

Current project documentation distinguishes:

- SQLite: mature, ordinary WAL still serializes writers
- libSQL: SQLite fork, still inherits fundamental single-writer model
- Turso Database: Rust rewrite, intended SQLite compatibility, with concurrent-write/MVCC work
- SQLite `BEGIN CONCURRENT`: experimental/non-trunk branch
- Turso's current local MVCC/compatibility path still requires careful validation before treating it as a production default

Sources:

- https://github.com/tursodatabase/libsql
- https://github.com/tursodatabase/turso
- https://github.com/tursodatabase/turso/blob/main/COMPAT.md
- https://github.com/tursodatabase/turso/blob/main/docs/manual.md
- https://turso.tech/blog/concurrent-writes-in-practice
- https://www.sqlite.org/hctree/doc/begin-concurrent/doc/begin_concurrent.md

---

# 76. Baseline architectural decision summary

```text
Language:              Rust

Live canonical store:  Turso Database from day one
Compatibility backend: SQLite
Concurrency mode:       Turso WAL initially; MVCC/BEGIN CONCURRENT when required capabilities are production-safe
Analytics/history:      DuckDB

Parsing baseline:      Rust-native parsers
Precision enrichment:  static repository metadata only; do not launch
                       analyzed-language compilers, LSPs, or runtimes

Graph execution:       Rust adjacency / CSR projections
Search:                SQLite FTS + structural ranking
Semantic search:       optional

Updates:               transactional and incremental
Daemon:                yes
MCP:                   first-class
Git awareness:         first-class
Provenance:            mandatory
LLM:                   optional, never required for code graph
Export:                derived, deterministic snapshots

Cloud model:            self-hosted workspace runtimes with embedded/local Turso; no Turso Cloud
Cloud scaling:          shard by workspace/repository owner
Extension model:         public stable SDK + namespaced semantics + in/out-of-process producers
Open-source boundary:    full general-purpose engine is public
First-party policy:      public integrations only for open-source STEXS projects
Private platform:        closed STEXS platform injects internal plugins through the same public SDK
STEXS integration:      Penelope mandatory full-engine reliability; public StateChronicle/Shardline/TrustGrant/stexs-bevy integrations
Runtime model:           embeddable full engine + lightweight runtime probes
Static/runtime model:    separate evidence layers with explicit trust/provenance
Penelope boundary:       mandatory full-engine workflows; absent from core/probes
StateChronicle role:     graph-history verification + authoritative committed-state observations
Multiplayer role:       engineering/control-plane intelligence; thin probes may observe runtime without entering authoritative gameplay logic

Public home:           STEXS-Technologies
Working name:          SyntaxMesh
```

The most important architectural boundary is:

> **Turso stores the live facts. Rust executes the graph. Penelope makes durable transitions reliable. StateChronicle can verify accepted history. DuckDB analyzes history. NDJSON exposes the graph to external tools and visualizers. SQLite remains the compatibility/reference backend.**

Keeping those three responsibilities separate gives the project a clean path from a fast local code index to a very large repository-intelligence platform without forcing one database or one representation to do every job.

---

# 77. v2 expansion thesis — from graph engine to evidence-backed system world model

The existing architecture is already deliberately more ambitious than a static code graph. It models source structure, schemas, infrastructure, documentation, runtime observations, Git history, architecture policy, provenance, and multi-repository relationships.

The next architectural step is **not** to replace that graph. It is to add a higher-level semantic and reasoning plane above it.

The target product model becomes:

```text
source / config / schemas / docs / Git / runtime / external adapters
                              |
                              v
                   deterministic fact plane
                              |
                              v
                  typed multi-layer graph
                              |
                +-------------+-------------+
                |                           |
                v                           v
       ontology / concept plane       historical analytics
                |
                v
       derivation / reasoning plane
                |
                v
     contracts / drift / explanations
                |
                v
   engineering decisions / context / plans
```

The fundamental distinction is:

```text
FACT PLANE
= what SyntaxMesh can directly extract, resolve, observe, or accept as an explicit declaration

KNOWLEDGE PLANE
= what those facts mean in a domain or organization

REASONING PLANE
= what follows from facts + ontology + declared rules/contracts
```

The graph remains the execution substrate.

The graph must **not** become the product boundary.

The long-term product should be understood as:

> **a continuously maintained, provenance-backed model of a software and engineering system that can explain what exists, what it means, why a claim is believed, what contradicts it, what would be affected by change, and which invariants must remain true.**

This expansion must preserve the original architecture:

- [ ] deterministic structural facts remain usable without ontology
- [ ] ontology is not required to index a repository
- [ ] inference never overwrites extracted facts
- [ ] an inferred relation is never silently presented as observed truth
- [ ] an LLM is never required for deterministic reasoning rules
- [ ] extension namespaces remain isolated
- [ ] every derived claim is reproducible from its inputs and rule version where the rule itself is deterministic
- [ ] the existing graph/query engine remains independently valuable
- [ ] no ontology feature is allowed to bloat the v0.1 foundation

---

# 78. First-class ontology subsystem

Add a dedicated semantic concept layer rather than treating domain semantics as only namespaced node/edge labels.

Suggested crate:

```text
syntaxmesh-ontology
```

This crate should depend on stable public graph/domain abstractions, not on daemon/HTTP/MCP hosts.

## 78.1 Ontology concepts

An ontology should be able to define concepts such as:

```text
core:ExecutableComponent
core:StatefulResource
core:PersistentDataStore
core:Authority
core:Operation
core:DataObject
core:ExternalSystem
core:TrustBoundary
core:SecurityControl
core:Workflow
core:UserIdentity
```

Extensions may introduce domain concepts:

```text
finance:FinancialLedger
finance:PaymentInstruction
finance:SettlementOperation

statechronicle:Resource
statechronicle:CommitAuthority

trustgrant:Capability
trustgrant:Issuer

bevy:System
bevy:Resource

acme:FraudPipeline
acme:RegionalDataStore
```

A graph node is an entity.

An ontology concept describes a semantic category or meaning that one or more entities may instantiate or represent.

Do not conflate:

```text
NodeKind::Struct
```

with:

```text
finance:FinancialLedger
```

One is structural syntax.

The other is domain meaning.

## 78.2 Core ontology relations

Provide a small, typed baseline vocabulary such as:

```text
INSTANCE_OF
SUBCLASS_OF
PART_OF
REPRESENTS
REALIZES
IMPLEMENTS_CONCEPT
EQUIVALENT_TO
SPECIALIZES
REQUIRES
PROVIDES
PRODUCES
CONSUMES
MUTATES
AUTHORIZES
PROTECTS
OWNS_AUTHORITY_FOR
CROSSES_BOUNDARY
```

Do not try to implement all of OWL/RDF/RDFS.

The goal is practical software/system reasoning, not standards maximalism.

Adapters may expose RDF/OWL interoperability later if there is real demand.

## 78.3 Concept schemas

Concepts and relations may define constraints:

```text
relation: MUTATES
subject domain: ExecutableComponent
object range: StatefulResource

relation: AUTHORIZES
subject domain: Authority | SecurityControl
object range: Operation
```

Use these constraints for:

- validation
- diagnostics
- query planning
- inference safety
- extension conformance

A malformed extension should not be able to register arbitrary self-contradictory schema without diagnostics.

## 78.4 Ontology namespaces

Use the existing extension namespace model.

Example:

```text
core://concept/StatefulResource
finance://concept/FinancialLedger
acme.payments://concept/SettlementRail
```

Namespace ownership rules must apply to:

- concepts
- relations
- constraints
- inference rules
- declared system models

## 78.5 Ontology versioning

Ontology identity must include a version/fingerprint.

A graph generation should be queryable under a specific semantic model:

```text
graph_generation = 5819
ontology_set      = [core@1, finance@4, acme.payments@9]
```

Changing ontology semantics may change derived knowledge even when structural facts do not change.

Therefore ontology changes need independent invalidation/versioning.

---

# 79. Deterministic inference and derivation engine

Add a restricted reasoning engine capable of deriving facts from explicit premises.

Suggested crate:

```text
syntaxmesh-inference
```

Do **not** begin by embedding a general Prolog runtime, full Datalog implementation, Cypher rule engine, or arbitrary user code execution.

Start with a constrained, inspectable rule model designed around SyntaxMesh entities and relations.

## 79.1 Example inference

Given:

```text
CheckoutHandler core:CALLS PaymentService
PaymentService core:WRITES LedgerDB
LedgerDB INSTANCE_OF finance:FinancialLedger
```

A deterministic rule may derive:

```text
CheckoutHandler finance:PARTICIPATES_IN_FINANCIAL_WRITE_PATH LedgerDB
```

Conceptually:

```text
CALLS+(x, y)
AND WRITES(y, store)
AND INSTANCE_OF(store, finance:FinancialLedger)
=> PARTICIPATES_IN_FINANCIAL_WRITE(x, store)
```

The result must record:

```text
derived fact ID
rule ID
rule version
premise fact IDs
ontology versions
source graph generation
configuration fingerprint
```

## 79.2 Restricted rule language

A future declarative syntax may look like:

```text
rule finance:financial_write_path {
    when
        $entry core:CALLS+ $writer
        $writer core:WRITES $store
        $store INSTANCE_OF finance:FinancialLedger

    derive
        $entry finance:PARTICIPATES_IN_FINANCIAL_WRITE $store
}
```

Required properties:

- deterministic where all premises/rules are deterministic
- bounded recursion/depth
- explicit relation/path constraints
- no arbitrary filesystem/network/process access
- no arbitrary code execution
- cycle detection
- resource budgets
- reproducible derivation IDs

## 79.3 Derivation classes

Keep at least:

```text
DETERMINISTIC_RULE
HEURISTIC_RULE
MODEL_INFERENCE
USER_DECLARED
RUNTIME_CORRELATION
```

Do not flatten all of these into `confidence = 0.83`.

A deterministic derivation may have no probabilistic confidence at all.

## 79.4 Materialized vs on-demand derivations

Not every possible inferred edge should be persisted.

Support two strategies:

```text
MATERIALIZED
- frequently queried
- cheap to invalidate incrementally
- useful for CI/policy

ON_DEMAND
- expensive
- highly contextual
- query-specific
```

Benchmark before materializing transitive closures at scale.

---

# 80. Truth-maintenance and claim state

SyntaxMesh should become explicit about the epistemic state of a claim.

A claim is not only an edge plus confidence.

Introduce a first-class model such as:

```text
Claim
ClaimStatus
Support
Contradiction
Derivation
EvidenceSet
```

Possible statuses:

```text
OBSERVED
EXTRACTED
RESOLVED
DECLARED
DERIVED
HEURISTIC
MODEL_INFERRED
CONTRADICTED
STALE
INVALIDATED
SUPERSEDED
```

## 80.1 Supporting and contradicting evidence

Example:

```text
Claim:
PaymentService only writes PaymentDB

supports:
- architecture declaration
- source dependency analysis

contradicts:
- runtime trace showing AnalyticsDB write
```

SyntaxMesh must retain both sides.

Do not erase the declaration because runtime evidence disagrees.

Do not erase the runtime evidence because the architecture says it should not happen.

The contradiction itself is useful intelligence.

## 80.2 Absence is not contradiction

Important invariant:

```text
STATICALLY_POSSIBLE edge exists
but runtime has never observed it
```

is **not** automatically a contradiction.

Likewise:

```text
no static edge found
but runtime observed call
```

may indicate:

- reflection
- dynamic dispatch
- generated code
- instrumentation ambiguity
- missing extractor support
- actual architecture drift

The system must express uncertainty instead of prematurely resolving it.

## 80.3 Incremental truth maintenance

When a premise disappears:

```text
Fact A removed
    -> derived claim B loses one support
    -> if no support remains, B becomes invalidated
    -> downstream derivations from B are invalidated
```

Do not rebuild all semantic knowledge globally after every file edit.

Maintain reverse dependency indexes from:

```text
premise fact -> derivations -> derived claims
```

---

# 81. Conceptual identity and entity-resolution layer

One of the highest-value capabilities is mapping many technical representations to the same domain concept.

Example physical representations:

```text
Rust struct UserId(Uuid)
TypeScript type UserId = string
OpenAPI UserIdentifier
protobuf UserRef
Postgres users.id
Kafka field user_id
```

SyntaxMesh should be able to represent:

```text
all REPRESENT ontology://company/user-identity
```

without pretending those technical entities are physically identical.

## 81.1 Three identity layers

Keep these distinct:

```text
PHYSICAL ARTIFACT IDENTITY
file/symbol/schema field/event field

LOGICAL SOFTWARE ENTITY IDENTITY
API operation / shared type / service / resource lineage

DOMAIN CONCEPT IDENTITY
customer identity / payment / entitlement / ledger / region
```

This allows one domain concept to have many implementations and encodings.

## 81.2 Resolution sources

Concept mapping may come from:

- deterministic schema links
- generated-code lineage
- package/build metadata
- explicit user declaration
- naming/type compatibility heuristics
- docs/ADR references
- optional LLM suggestion

Every mapping retains its derivation class.

## 81.3 Never auto-promote uncertain equivalence

An LLM suggesting:

```text
CustomerRef probably means CustomerId
```

must not become a canonical equivalence silently.

Possible lifecycle:

```text
MODEL_INFERRED candidate
      -> evidence accumulation
      -> user/tool confirmation
      -> DECLARED / RESOLVED concept mapping
```

## 81.4 Cross-repository conceptual queries

Target queries:

```text
Show every technical representation of customer identity.

Where does the concept "payment authorization" exist across repositories?

Which APIs, events and tables expose the same business concept?

Where do two services disagree on the representation of Money?
```

This is a key step from repository graph to system intelligence.

---

# 82. First-class contracts, invariants and semantic constraints

The existing architecture-rule engine should remain, but add a richer semantic contract model.

Potential concepts:

```text
Contract
Invariant
Precondition
Postcondition
CompatibilityConstraint
StateTransitionConstraint
SecurityInvariant
DataResidencyConstraint
AvailabilityConstraint
OwnershipConstraint
```

## 82.1 Examples

```text
Money.amount MUST_BE >= 0

Order.PAID REQUIRES Payment.AUTHORIZED

PublicApiV2 MUST_REMAIN_COMPATIBLE_WITH PublicApiV1

LedgerWrite REQUIRES Capability(finance:ledger-write)

UserDeletion MUST_EVENTUALLY_REMOVE PII(User)

EU_PII MUST_NOT_FLOW_TO region:non_eu
```

SyntaxMesh does not execute the business process.

It understands the contract and can compare it to source/runtime/model evidence.

## 82.2 Contract sources

Contracts may be:

```text
SOURCE_DERIVED
SCHEMA_DERIVED
CONFIG_DERIVED
DECLARED
DOCUMENTED
RUNTIME_INFERRED
MODEL_SUGGESTED
```

Only the first four should normally be eligible for hard CI enforcement without explicit confirmation/policy.

## 82.3 Relation to architecture rules

Architecture rule:

```text
domain MUST NOT depend on http
```

Semantic contract:

```text
all mutation paths for FinancialLedger MUST pass AuthorizationBoundary
```

Both should use a common evidence/result framework, but they are different abstractions.

---

# 83. Data-flow, lineage, effects and taint semantics

The graph currently models calls/dependencies/reads/writes. Add a richer data/effect layer.

Potential core relations:

```text
DATA_FLOWS_TO
DERIVES_FROM
TRANSFORMS_TO
SANITIZES
VALIDATES
ENCODES
DECODES
ENCRYPTS
DECRYPTS
AUTHENTICATES
AUTHORIZES
PERSISTS
EMITS
EXPOSES
LOGS
CROSSES_TRUST_BOUNDARY
CROSSES_REGION
```

## 83.1 Data lineage example

```text
HTTP request.email
    DATA_FLOWS_TO RequestDto.email
    DATA_FLOWS_TO User.email
    DATA_FLOWS_TO users.email
    DATA_FLOWS_TO AnalyticsEvent.email
```

Now SyntaxMesh can answer:

```text
Where can PII leave the request boundary?

Which APIs can ultimately affect this table column?

Can this secret reach logging?

Which data crosses a regional boundary?

Where does untrusted input become validated?
```

## 83.2 Effect model

Functions/components may expose effects:

```text
READS_STATE
MUTATES_STATE
EMITS_EVENT
PERFORMS_NETWORK_IO
WRITES_FILE
SPAWNS_PROCESS
USES_SECRET
PRIVILEGED_OPERATION
```

Effects can be extracted deterministically where possible and inferred conservatively otherwise.

## 83.3 Security/taint analysis boundary

SyntaxMesh should support useful taint-like path semantics without trying to immediately replace mature SAST products.

Start with:

- configurable source classes
- sanitizer/validator relations
- sink classes
- trust-boundary crossings
- provenance-backed path explanation

Do not claim sound whole-program taint analysis for dynamic languages unless the analyzer actually provides it.

---

# 84. Behavioral model and named system flows

A system is not only a set of dependencies. It performs operations.

Introduce first-class behavior concepts:

```text
Flow
Step
Transition
Trigger
Guard
Effect
FailurePath
Compensation
Boundary
Participant
```

Example:

```text
Flow: user-login

Client
 -> Gateway
 -> Authentication
 -> AccountLookup
 -> SessionCreation
 -> TokenIssue
```

Another:

```text
Flow: purchase

API
 -> authorization
 -> inventory reservation
 -> payment capture
 -> ledger mutation
 -> entitlement grant
 -> event publication
```

## 84.1 Flow discovery

A flow may be:

```text
DECLARED
DISCOVERED_STATICALLY
OBSERVED_RUNTIME
HYBRID
```

A declared flow should be comparable against actual source/runtime evidence.

## 84.2 Behavior != workflow execution

SyntaxMesh must not become Penelope.

SyntaxMesh understands:

```text
what the process is
which components participate
which state/resources it touches
what contracts apply
what was observed
```

Penelope executes/orchestrates durable workflows.

Keep that boundary explicit.

## 84.3 Named behavior queries

Target queries:

```text
Show the login flow.

Which components participate in refund settlement?

Where can purchase fail after payment capture?

Which compensations exist after inventory reservation?

Which runtime path is actually used for session refresh?
```

---

# 85. Declarative system model — intended architecture as data

Source analysis can only discover what exists.

SyntaxMesh should also accept what the organization says **should** exist.

Provide a versioned declarative model.

Example YAML/TOML is illustrative; canonical representation should use the typed public model.

```yaml
concept: payment-ledger
kind: finance:FinancialLedger

implemented_by:
  - crate://ledger

invariants:
  - append_only
  - writes_require: finance:LedgerWrite

classification:
  - financial
  - regulated

region:
  - eu
```

## 85.1 Intended / implemented / observed

Maintain three distinct realities:

```text
INTENDED
= declarations, ADRs, architecture model, policy

IMPLEMENTED
= deterministic source/config/schema/build facts

OBSERVED
= runtime/test/telemetry facts
```

Never collapse these into one graph relation without origin.

## 85.2 Declarations are not source facts

A user declaration is authoritative about intent, not necessarily reality.

For example:

```text
DECLARED:
Gateway never writes LedgerDB directly
```

If source/runtime disagrees, SyntaxMesh should report drift rather than modifying the declaration.

---

# 86. Architectural drift detection

Once intended, implemented and observed models coexist, compare them continuously.

Example:

```text
INTENDED:
Gateway -> Authorization -> Ledger

IMPLEMENTED:
Gateway -> Authorization -> Ledger
Gateway -> Ledger

OBSERVED:
Gateway -> Ledger
```

Result:

```text
Drift:
- undeclared direct mutation path exists
- direct path violates authorization invariant
- direct path observed at runtime
```

## 86.1 Drift classes

Potential classifications:

```text
MISSING_IMPLEMENTATION
UNDECLARED_DEPENDENCY
UNDECLARED_RUNTIME_PATH
BROKEN_CONTRACT
STALE_DOCUMENTATION
UNUSED_DECLARED_COMPONENT
POLICY_VIOLATION
SCHEMA_DRIFT
DEPLOYMENT_DRIFT
SECURITY_BOUNDARY_DRIFT
```

## 86.2 Drift severity is policy, not ontology truth

SyntaxMesh may expose facts such as:

```text
runtime path bypasses declared boundary
```

Whether that is warning/error/blocker belongs to configured policy.

---

# 87. Semantic diff

Extend the existing graph diff into semantic change analysis.

The hierarchy becomes:

```text
text diff
    -> symbol diff
        -> graph diff
            -> semantic diff
```

Examples:

```text
AUTHORITY_FOR(refund):
PaymentService -> RefundCoordinator
```

```text
PII residency:
EU_ONLY -> EU_AND_US
```

```text
authorization requirement:
AdminCapability -> UserCapability
```

```text
concept representation:
Money(v1) -> Money(v2)
```

## 87.1 Semantic diff inputs

Compare two complete semantic worlds:

```text
GraphGeneration A + OntologySet A + DeclarationSet A
vs
GraphGeneration B + OntologySet B + DeclarationSet B
```

Do not report semantic changes without indicating whether they came from:

- source change
- runtime evidence change
- ontology/rule upgrade
- declaration/policy change
- plugin version change

## 87.2 PR intelligence

PR analysis should eventually report:

```text
structural changes
behavioral changes
contract changes
security-boundary changes
data-flow changes
domain-concept changes
architecture drift introduced/resolved
```

This should remain evidence-backed.

---

# 88. Counterfactual / hypothetical analysis overlays

Support reasoning over proposed changes without mutating canonical state.

Model:

```text
canonical graph generation N
          |
          +-- hypothetical overlay H
                     |
                     v
            affected derivations
                     |
                     v
           impact / violations / plan
```

Potential questions:

```text
What if this API field is removed?

What if this service moves to another region?

What if this DB becomes append-only?

What if this crate stops being public?

What if authorization moves behind Gateway X?
```

## 88.1 Overlay requirements

- ephemeral by default
- deterministic where the proposed delta and rules are deterministic
- separate stable overlay identity
- no writes to canonical graph unless explicitly committed through normal indexing/declaration paths
- bounded derivation depth
- clear distinction between hypothetical and actual facts

## 88.2 Use cases

```text
architecture design review
migration planning
breaking-change analysis
security review
schema evolution
service decomposition
regionalization
API compatibility planning
```

---

# 89. Proof and explanation trees

Every significant derived claim should be explainable through a derivation DAG.

Example:

```text
Claim:
CheckoutHandler participates in regulated financial write path

because:
1. CheckoutHandler CALLS PaymentService
2. PaymentService WRITES LedgerDB
3. LedgerDB INSTANCE_OF finance:FinancialLedger
4. finance:FinancialLedger SUBCLASS_OF finance:RegulatedFinancialStore
5. deterministic rule finance:regulated_write_path@3
```

Expose through:

```text
syntaxmesh explain <claim-id>
syntaxmesh explain --why "CheckoutHandler is in payment boundary"
MCP explain_claim
HTTP /claims/{id}/explanation
```

## 89.1 Explanation requirements

Include:

- conclusion
- derivation type
- rule/version
- premise claims/facts
- evidence spans
- graph generation
- ontology set
- extension/plugin identities
- uncertainty where present
- contradiction state where present

An LLM may summarize the explanation but must not be the source of the proof unless the claim itself is explicitly `MODEL_INFERRED`.

---

# 90. Contradiction and anomaly discovery

Run bounded consistency analyses across facts, ontology, declarations, contracts and runtime observations.

Potential findings:

```text
two components both declared exclusive authority for same resource

resource declared local-only but exported through public API

service declared stateless but observed writing persistent state

documentation says sync path but runtime/source uses event bus

API schema says non-null but observed payload contains null

security-sensitive sink reachable without known authorization boundary

two repositories encode same domain concept incompatibly
```

## 90.1 Findings are claims with evidence

Do not create a special opaque warning system.

An anomaly should reference:

```text
involved entities
supporting evidence
conflicting evidence
applicable contract/rule
semantic generation
severity policy
```

## 90.2 Avoid overclaiming

Examples:

```text
"not observed" != "impossible"
"not statically resolved" != "does not exist"
"LLM suggests mismatch" != "contract violation"
```

SyntaxMesh should prefer precise epistemic wording.

---

# 91. Evidence-backed change requirements and refactoring intelligence

Long-term, SyntaxMesh should move beyond ordinary impact analysis, but it must stop before executable transformation planning.

Target question:

```text
What must remain true, and what parts of the system are implicated, if CustomerId semantics change?
```

A structured SyntaxMesh result may include:

```text
affected concepts and representations
affected schemas and persistence
compatibility obligations
affected producers and consumers
build/deploy relationships
covered and uncovered tests
architecture/security contracts
runtime-critical paths
historical migration evidence
required post-change predicates
unknowns / contradictions / missing evidence
```

SyntaxMesh should derive this from:

```text
graph dependencies
concept mappings
contracts
schema lineage
build/deploy relationships
test coverage
runtime evidence
compatibility rules
Git history
incident/decision history
```

An LLM may summarize or suggest candidate implementation tactics, but it must not invent the dependency skeleton or silently convert uncertain evidence into a mandatory step.

## 91.1 Change-requirements representation

Prefer typed outputs:

```text
ChangeIntent
ChangeRequirement
AffectedEntity
AffectedConcept
Constraint
Precondition
Postcondition
VerificationPredicate
CompatibilityRequirement
RiskFinding
Evidence
Unknown
```

This lets CI/IDEs/agents and higher-level tooling consume the result without parsing prose.

## 91.2 Explicit boundary with transformation tooling

SyntaxMesh does **not** own:

```text
ordered executable migration plans
source-code mutation
AST rewrite execution
shell commands
deployment execution
database migration execution
Git commits / pull-request creation
rollback execution
```

Those belong to a separate tool layered above SyntaxMesh. That tool may use SyntaxMesh's change requirements and verification predicates to create a concrete plan, execute transformations through specialized providers, re-index the changed system, and ask SyntaxMesh whether the required semantic postconditions now hold.

SyntaxMesh therefore remains an intelligence and verification substrate, not a mutation orchestrator.

---

# 92. Extension SDK expansion for semantic intelligence

The existing extension system is already the right boundary. Extend it rather than creating a privileged internal semantic API.

Additional extension capabilities may include:

```text
OntologyProvider
ConceptMapper
InferenceRuleProvider
ContractProvider
FlowProvider
DataClassificationProvider
EffectAnalyzer
EntityResolver
SemanticQueryProvider
DriftAnalyzer
```

Conceptually:

```rust
pub trait OntologyProvider {
    fn ontology_manifest(&self) -> &OntologyManifest;
    fn register_concepts(&self, sink: &mut dyn OntologySink) -> Result<(), Error>;
    fn register_relations(&self, sink: &mut dyn OntologySink) -> Result<(), Error>;
}

pub trait InferenceRuleProvider {
    fn rules(&self) -> Result<Vec<InferenceRule>, Error>;
}

pub trait ContractProvider {
    fn contracts(&self) -> Result<Vec<SemanticContract>, Error>;
}
```

Do not make extension providers return executable closures across an out-of-process trust boundary.

External extensions should transmit declarative, versioned models/rules through the public protocol.

## 92.1 Extension security

For out-of-process semantic extensions:

- no arbitrary code execution inside SyntaxMesh
- no arbitrary SQL
- no unrestricted filesystem paths
- no generic URL fetching controlled by extension data
- bounded rule complexity
- bounded ontology size
- bounded derivation fanout
- explicit namespace ownership
- explicit producer identity/version

Trusted in-process Rust integrations are host-controlled code and therefore follow the host's ordinary trust model.

---

# 93. Storage model additions

The live canonical store should gain typed tables/relations for semantic intelligence rather than encoding everything into opaque JSON.

Potential logical entities:

```text
ontology_namespaces
ontology_versions
concepts
concept_relations
entity_concept_mappings
claims
claim_support
claim_contradictions
derivations
inference_rules
rule_versions
contracts
contract_versions
flows
flow_steps
flow_relations
data_classifications
effects
declarations
declaration_versions
semantic_overlays
semantic_diff_results
change_plans
```

Do not commit to this physical schema before query benchmarks.

The requirement is a stable logical model behind storage traits.

## 93.1 Separate source graph state from derived semantic state

A useful conceptual separation:

```text
canonical fact tables
        |
        +--> derived semantic materializations
        |
        +--> analytical/historical projections
```

A failed semantic rebuild must not corrupt the structural graph.

## 93.2 Rebuildability

Derived knowledge should be disposable/rebuildable from:

```text
canonical facts
declarations
ontology/rule versions
plugin versions
configuration
```

wherever the derivation is deterministic.

LLM/model-derived claims require their recorded result/provenance if exact replay is desired.

---

# 94. Semantic generation identity

Graph generation alone is no longer enough once ontology/rules/declarations affect answers.

Define a higher-level semantic generation/fingerprint.

Conceptually:

```text
SemanticGeneration {
    graph_generation,
    ontology_set_hash,
    inference_rule_set_hash,
    declaration_set_hash,
    contract_set_hash,
    semantic_config_hash,
}
```

Every semantic query result should be attributable to this identity.

This allows:

- reproducible CI
- semantic diff
- cache invalidation
- historical explanation
- StateChronicle verification of semantic analysis artifacts if desired

Do not force StateChronicle to record each claim.

If verified semantic history is needed, bind deterministic semantic-generation manifests/results at an appropriate publication boundary.

---

# 95. Incremental semantic invalidation

Do not turn ontology/inference into a global recomputation tax.

Track dependencies explicitly:

```text
source fact
 -> concept mapping
 -> derived claim
 -> contract evaluation
 -> drift finding
 -> change requirements
```

Changes should invalidate only downstream knowledge whose premises changed.

Examples:

```text
private function body changed
 -> no exported semantic surface change
 -> do not invalidate unrelated ontology mappings

public type renamed but stable continuity detected
 -> identity preserved
 -> re-evaluate representation mappings only where names mattered

ontology rule upgraded
 -> invalidate derivations produced by that rule/version
```

Benchmark:

- full semantic build
- single-file semantic update
- ontology version upgrade
- declaration edit
- runtime observation batch
- contract rule update

---

# 96. Query engine evolution

Keep deterministic graph query primitives, then add semantic operations.

Potential query families:

```text
ConceptInstances(concept)
Representations(concept)
Why(claim)
Supports(claim)
Contradicts(claim)
Infer(pattern)
Contracts(entity)
Violations(contract)
Flows(entity_or_concept)
DataLineage(source, sink_class)
Effects(entity)
Drift(scope)
SemanticDiff(a, b)
Counterfactual(base, overlay)
ChangeRequirements(target_change)
```

The natural-language planner can map user questions into these typed operations.

Execution stays deterministic where the underlying operations are deterministic.

Examples:

```text
"where is customer identity represented?"
 -> Representations(company:CustomerIdentity)

"why is this service considered part of payments?"
 -> Why(claim)

"can PII reach analytics?"
 -> DataLineage(PII, analytics sinks)

"what changed architecturally in this PR?"
 -> SemanticDiff(base, head)
```

---

# 97. Context compiler evolution

The context compiler should exploit the semantic plane.

A coding agent should receive not only nearby code, but the minimum evidence needed to understand relevant meaning and constraints.

Possible context pack sections:

```text
TASK TARGET
relevant symbols/files

DOMAIN CONCEPTS
concepts represented by those symbols

BEHAVIOR
named flows affected

CONTRACTS / INVARIANTS
rules that must remain true

IMPACT
callers/dependents/data lineage/tests/deployments

EVIDENCE
exact source spans and derivation paths

AMBIGUITIES / CONTRADICTIONS
facts not fully resolved

HISTORICAL CONTEXT
relevant ADRs/commits/previous changes
```

Token budgeting should score semantic uniqueness and invariant relevance, not only graph proximity.

---

# 98. Historical semantic intelligence

DuckDB history should eventually analyze semantic changes, not only graph metrics.

Potential questions:

```text
When did this service become part of the payment boundary?

When did PII first begin flowing to this subsystem?

How often has ownership of this domain concept moved?

Which invariants repeatedly regress?

Which declared architecture areas drift most often?

Which concept representations have accumulated the most incompatibility?

How has the login flow changed across releases?
```

Historical semantic analysis must record the ontology/rule version used at each point or provide a clearly labeled "re-evaluated under current ontology" mode.

These are different questions:

```text
What did SyntaxMesh conclude then?
```

vs

```text
What would current semantics conclude about old source state?
```

Do not conflate them.

---

# 99. Ontology and reasoning interoperability

Do not make external ontology standards mandatory internally.

Possible future adapters:

```text
RDF export/import
RDFS subset mapping
OWL subset mapping
SHACL-like constraint import
Datalog adapter
JSON-LD export
```

Only add these where ecosystem demand is concrete.

The internal model should remain designed for software/system intelligence, stable IDs, provenance, incremental invalidation and high-performance graph execution.

---

# 100. LLM role in the semantic world model

LLMs are useful, but must remain subordinate to evidence and typed semantics.

Good LLM uses:

```text
propose concept mappings
summarize derivation/proof trees
suggest ontology candidates
classify documentation
suggest missing contracts
translate natural language into typed queries
explain semantic diff
propose likely flow names
```

Bad uses:

```text
invent dependencies
invent proof paths
silently assert equivalence
replace deterministic graph resolution
promote model guesses into hard CI policy
```

## 100.1 Promotion workflow

For high-value semantic guesses:

```text
MODEL_SUGGESTED
      |
      v
EVIDENCE_SUPPORTED
      |
      v
USER_CONFIRMED / TOOL_CONFIRMED
      |
      v
DECLARED or RESOLVED
```

Not every model suggestion needs promotion.

## 100.2 Model independence

A repository indexed without an LLM must remain fully queryable structurally.

Semantic features that depend on model-produced mappings must degrade explicitly when those mappings are absent.

---

# 101. Security and trust model for semantic reasoning

Adding reasoning creates new attack surfaces.

Threats include:

```text
malicious repository causes rule explosion
malicious extension registers pathological ontology
cyclic derivation fanout
adversarial docs manipulate LLM semantic enrichment
untrusted runtime observation spoofs architecture evidence
model-generated false equivalence
oversized declarative model exhausts memory
```

Required mitigations:

- derivation budgets
- recursion/path depth caps
- rule static validation
- cycle detection
- namespace quotas
- ontology size caps
- bounded result/materialization counts
- source trust classification
- runtime observer trust classification
- no arbitrary executable rule code from untrusted inputs
- provenance everywhere
- user-visible distinction between trusted declaration and untrusted observation
- prompt-injection-resistant separation between repository text and control instructions in semantic/LLM jobs

Do not treat repository documentation as instructions to SyntaxMesh's model provider.

Repository content is data.

---

# 102. Performance model for the reasoning plane

The semantic world model must not make ordinary code navigation slow.

Maintain separate latency classes:

```text
HOT STRUCTURAL
symbol lookup
neighbors
path
impact
FTS

WARM SEMANTIC
concept lookup
contract lookup
materialized derived claims
flow lookup

COLD ANALYTICAL
large ontology recomputation
counterfactual simulation
semantic diff across large histories
complex data-lineage analysis
change-requirements generation
```

Do not put cold semantic work in ordinary editor keystroke paths.

Precompute/materialize only where measurements justify it.

---

# 103. Benchmark expansion

Add a third benchmark family beside engine performance and agent usefulness:

## Semantic intelligence benchmarks

Measure:

```text
concept mapping precision/recall
entity-resolution precision/recall
derivation correctness
explanation completeness
invalidation correctness
semantic diff accuracy
drift-detection accuracy
data-lineage precision/recall
contract violation precision/recall
counterfactual consistency
```

Datasets should include known ground-truth systems with:

- multiple languages
- duplicated domain concepts
- API/schema/event representations
- declared architecture
- deliberate architecture drift
- runtime traces
- known contract violations
- historical refactors

The goal is not simply more edges.

The goal is better engineering answers.

---

# 104. Roadmap placement — do not bloat v0.1

The semantic world-model architecture should be reserved now, but implementation should remain staged.

## v0.1 foundation remains unchanged in spirit

Required:

```text
transactional incremental fact graph
stable IDs
provenance
Rust/TypeScript/Python extraction
search/path/impact
Git awareness
MCP/context compiler
extension SDK
Penelope reliability
StateChronicle verified-history support
DuckDB analytical export
```

Do not block v0.1 on ontology/inference.

## Early post-v0.1 / pre-v1 primitives

Add only the primitives necessary to avoid future schema/API breakage:

```text
ClaimId / FactId distinction
semantic generation identity
extension capability reservation for ontology/rules/contracts
source/declaration/inference provenance classes
versioned concept/relation identifiers
public DTO hooks for concept mappings
```

## v1-era candidates

High-value additions:

```text
ontology registry
entity-to-concept mappings
contracts/invariants
deterministic derivation DAGs
proof/explanation trees
intended vs implemented vs observed model
architecture drift
semantic diff
```

## Post-v1 intelligence expansion

More advanced:

```text
data-flow/effect analysis
behavioral flow discovery
counterfactual overlays
cross-org ontology interoperability
anomaly discovery
change-requirements generation
advanced semantic history
```

The exact release assignment should be driven by implementation pressure and user demand.

---

# 105. Five primitives that must remain architecturally possible

Even if most v2 semantic capabilities are deferred, the current implementation must not make these hard to add later:

```text
1. ONTOLOGY
   stable concepts and semantic relations

2. DERIVED FACTS / INFERENCE
   reproducible claims with explicit premises

3. ENTITY RESOLUTION
   many technical representations mapped to one logical/domain concept

4. CONTRACTS / INVARIANTS / EFFECTS
   explicit semantics beyond graph topology

5. TRUTH MAINTENANCE
   support, contradiction, invalidation and derivation history
```

These five primitives unlock nearly every higher-level capability in sections 77-104.

---

# 106. Updated product positioning

Avoid positioning SyntaxMesh primarily as:

> a graph database for code

or:

> Graphify with a faster backend and plugins

The stronger long-term positioning is:

> **SyntaxMesh is a local-first, incremental, provenance-backed source and system intelligence engine that maintains an evidence-based model of how software is structured, what system concepts mean, how behavior flows across code and infrastructure, which contracts must hold, and what changes imply.**

A shorter form:

> **SyntaxMesh turns software repositories and runtime evidence into a continuously maintained engineering world model.**

The graph is an internal representation and public query/export surface.

The differentiator is the **quality and explainability of system knowledge** built on top of it.

---

# 107. Updated responsibility model

```text
Turso / compatible live store
= canonical current facts, declarations, semantic metadata and materialized knowledge

Rust graph engine
= high-performance traversal, structural algorithms and projections

Ontology layer
= domain/system meaning and concept relationships

Inference engine
= reproducible derived knowledge from explicit premises/rules

Truth-maintenance layer
= support, contradiction, invalidation and derivation dependencies

Contract/drift engine
= intended-vs-implemented-vs-observed comparison and invariant evaluation

Context/query layer
= deterministic engineering questions, evidence packs and agent/tool APIs

Penelope
= reliable durable multi-step transitions/rebuilds/enrichment/migrations

StateChronicle
= optional verification of accepted/published graph or semantic-generation history

DuckDB
= historical, trend and large analytical computation

NDJSON / public DTOs
= stable external interchange and tooling boundary
```

Keep these responsibilities separate even when implemented in the same process.

---

# 108. Updated final architectural decision summary

```text
Language:               Rust

Live canonical store:   Turso Database from day one
Compatibility backend:  SQLite
Concurrency mode:       Turso WAL initially; MVCC/BEGIN CONCURRENT when required capabilities are production-safe
Analytics/history:      DuckDB

Parsing baseline:       Rust-native parsers
Precision enrichment:   static repository metadata only; do not launch
                        analyzed-language compilers, LSPs, or runtimes

Fact model:             typed, stable-ID, provenance-first, incremental
Graph execution:        Rust adjacency / CSR projections
Search:                 SQLite FTS + structural ranking
Semantic search:        optional

Knowledge model:        optional first-class ontology/concept plane above graph facts
Inference model:        deterministic restricted rules first; heuristic/model derivations explicitly separated
Truth model:            facts, claims, supports, contradictions, derivations, invalidation
Identity model:         physical entity -> logical entity -> domain concept
Contracts:              architecture rules + semantic invariants + compatibility/security/data constraints
Behavior:               named flows/effects/data lineage without becoming workflow execution
Change reasoning:       graph diff -> semantic diff -> counterfactual impact -> evidence-backed change requirements

Updates:                transactional and incremental
Semantic invalidation:  dependency-aware; never global by default
Daemon:                 yes
MCP:                    first-class
Git awareness:          first-class
Provenance:             mandatory
LLM:                    optional; never required for structural graph or deterministic rule execution
Export:                 derived, deterministic snapshots/NDJSON

World-model states:     intended / implemented / observed remain distinct
Claim states:           observed/extracted/declared/derived/inferred/contradicted/stale/invalidated
Explanations:           derivation DAG + exact evidence; LLM may summarize but not fabricate proof

Cloud model:            self-hosted workspace runtimes with embedded/local Turso; no Turso Cloud requirement
Cloud scaling:          shard by workspace/repository owner
Extension model:        public stable SDK + namespaced facts + ontology/rule/contract providers + in/out-of-process producers
Open-source boundary:   full general-purpose engine and semantic extension interfaces are public
First-party policy:     public integrations only for open-source STEXS projects
Private platform:       closed STEXS platform injects private semantics through the same public SDK

STEXS integration:      Penelope for durable workflows; StateChronicle for optional verified history; Shardline for immutable artifacts; TrustGrant/state adapters as domain extensions
Runtime model:          embeddable full engine + lightweight runtime probes
Static/runtime model:   separate evidence layers with explicit trust/provenance
Security model:         declarative bounded inference; no untrusted arbitrary rule code/RCE

v0.1 policy:            ship the reliable incremental graph foundation first
v1+ policy:             add ontology/reasoning where real product pressure proves value
Long-term category:     evidence-backed engineering world model and reasoning engine

Public home:            STEXS-Technologies
Working name:           SyntaxMesh
```

The critical long-term invariant is now:

> **SyntaxMesh must never blur observation, declaration, inference and speculation. It becomes more powerful by connecting those categories through explicit evidence and derivation—not by pretending they are the same kind of truth.**

The architectural progression should therefore remain:

```text
source/system facts
        -> typed live graph
        -> ontology/concept model
        -> deterministic derivations
        -> contracts and truth maintenance
        -> semantic/historical reasoning
        -> evidence-backed engineering decisions
```

This preserves the original fast, reliable source-intelligence foundation while giving SyntaxMesh a path to become substantially more than a graph database: a continuously updated, explainable model of an engineering system and its semantics.

---

# 109. v3 design rule — SyntaxMesh absorbs engineering knowledge, not every engineering product

The v3 expansion deliberately harvests useful capabilities from adjacent tool categories while preserving a strict responsibility boundary.

SyntaxMesh SHOULD absorb capabilities that improve:

```text
fact precision
semantic meaning
cross-source reconciliation
system modeling
explainability
change impact
verification
agent context quality
historical engineering intelligence
```

SyntaxMesh SHOULD NOT absorb the operational products that generate or act on that evidence.

Examples:

```text
compiler/indexer       -> SyntaxMesh ingests precision facts
SAST engine            -> SyntaxMesh ingests data/control-flow evidence
APM/telemetry backend  -> SyntaxMesh ingests summarized runtime evidence
software catalog       -> SyntaxMesh ingests declared ownership/system facts
EA repository          -> SyntaxMesh ingests intended architecture/scenarios
incident system        -> SyntaxMesh ingests problem/root-cause lineage
rewrite engine         -> upper-layer change tool executes transformations
CI/CD                  -> executes builds/tests/deployments; SyntaxMesh models results
```

This produces a durable architectural principle:

> **SyntaxMesh is the semantic reconciliation and reasoning layer. It should understand evidence from specialized tools without trying to replace every specialized tool.**

---

# 110. Precision-analysis provider framework

Tree-sitter remains an excellent deterministic baseline, but SyntaxMesh must be able to consume more authoritative semantic facts when a compiler, language server, build system, external indexer, or specialized analyzer can provide them.

Do not force every language's deepest semantics through one home-grown resolver.

Introduce a provider abstraction such as:

```rust
pub trait AnalysisProvider: Send + Sync {
    fn descriptor(&self) -> &AnalysisProviderDescriptor;
    fn capabilities(&self) -> AnalysisCapabilities;
    fn analyze(&self, request: AnalysisRequest, sink: &mut dyn AnalysisFactSink)
        -> Result<(), AnalysisProviderError>;
}
```

Potential provider classes:

```text
PARSER
COMPILER
LSP
SEMANTIC_INDEX
BUILD_SYSTEM
SCHEMA_ANALYZER
PROGRAM_ANALYSIS
SECURITY_ANALYSIS
RUNTIME_SUMMARY
CATALOG
INCIDENT_HISTORY
CUSTOM
```

## 110.1 Precision facts do not overwrite baseline facts

Example:

```text
Tree-sitter:
    foo CALLS bar
    certainty = STATICALLY_RESOLVED

compiler index:
    foo CALLS crate::module::bar#symbol-id
    certainty = COMPILER_RESOLVED
```

SyntaxMesh may prefer the compiler-resolved fact for a query, but it should retain provenance and reconciliation metadata rather than silently deleting the original evidence.

Useful relation:

```text
Fact B REFINES Fact A
Fact C CONTRADICTS Fact A
Fact D CONFIRMS Fact A
```

## 110.2 Provider authority is capability-specific

Do not create one global ranking such as:

```text
compiler > runtime > user
```

Authority depends on the claim.

Examples:

```text
symbol target resolution:
    compiler/indexer usually stronger than heuristic parser resolution

actual production invocation:
    runtime observation stronger than static possibility

intended ownership:
    explicit architecture/catalog declaration stronger than Git frequency

implemented ownership:
    source/config evidence may contradict declared ownership
```

The query planner should reason about evidence class, not one universal confidence number.

## 110.3 SCIP-like semantic-index interoperability

Support import adapters for established semantic-index interchange formats where useful.

The internal representation remains SyntaxMesh-native.

Imported precision records should map into:

```text
symbol identity
reference identity
definition/reference links
implementation links
type information
documentation
package/module identity
source spans
producer/version provenance
```

Never make an external format the canonical database schema.

## 110.4 Analyzer result reproducibility

Record:

```text
provider ID
provider version
analysis format/version
configuration fingerprint
toolchain/compiler version
input content hashes
workspace/build profile
feature flags / target triple where relevant
```

A Rust analysis under one feature set may not equal another.

---

# 111. Program-analysis overlays

The graph should support richer program semantics than ordinary call/import relationships.

Potential optional overlays:

```text
AST / syntax structure
control-flow graph (CFG)
call graph
data-flow graph
def-use graph
control dependence
data dependence
dominance / post-dominance
taint propagation
effect graph
exception/error propagation
allocation/resource lifetime relationships
concurrency/synchronization relationships
```

These SHOULD be produced either by SyntaxMesh analyzers or external specialized providers.

## 111.1 Overlay identity

Every overlay must state:

```text
analysis kind
scope
provider
provider version
source graph generation
configuration/toolchain
soundness/completeness claims
known limitations
```

Example:

```text
analysis_kind = TAINT_FLOW
language = Java
provider = external-codeql-adapter
scope = repository
completeness = conservative-with-documented-gaps
```

Do not market all analysis providers as equally complete.

## 111.2 Paths become evidence objects

A data-flow or taint path should be a first-class evidence object rather than merely a list of nodes:

```text
AnalysisPath {
    kind,
    source,
    sink,
    hops,
    guards,
    sanitizers,
    call_context,
    provider,
    provenance,
}
```

This makes security findings and semantic contracts explainable.

## 111.3 Program analysis feeds ontology reasoning

Example:

```text
external analyzer:
HTTP.body.email DATA_FLOWS_TO AnalyticsEvent.email

ontology:
HTTP.body.email INSTANCE_OF PII.Email
AnalyticsEvent INSTANCE_OF ExternalTelemetry

contract:
PII MUST_NOT_FLOW_TO ExternalTelemetry
```

SyntaxMesh can then evaluate the semantic contract using a precise program-analysis path without reimplementing the full analysis engine itself.

---

# 112. Ontology interfaces, capabilities and semantic shapes

The ontology model should support reusable semantic interfaces in addition to ordinary concept inheritance.

Example interfaces:

```text
core:PersistentStore
core:AuthoritativeState
core:ExternalEndpoint
core:AuthenticatedEndpoint
core:EventProducer
core:EventConsumer
core:SecretSource
core:PIIContainer
core:RegionalResource
core:IdempotentOperation
core:CompensatableOperation
core:UserFacingOperation
```

Concrete concepts may implement multiple interfaces:

```text
postgres:Table
    IMPLEMENTS_INTERFACE core:PersistentStore

statechronicle:Resource
    IMPLEMENTS_INTERFACE core:AuthoritativeState

payments:CapturePayment
    IMPLEMENTS_INTERFACE core:IdempotentOperation
    IMPLEMENTS_INTERFACE finance:MoneyMutation
```

## 112.1 Why interfaces matter

Rules should be reusable across technologies.

Bad:

```text
Postgres table X requires encryption
MySQL table Y requires encryption
Dynamo table Z requires encryption
```

Better:

```text
all PersistentStore containing PII
MUST_HAVE encryption-at-rest control
```

## 112.2 Interface requirements

An ontology interface may define:

```text
required properties
required relations
optional properties
allowed relation targets
semantic constraints
capabilities
```

Example:

```text
interface AuthoritativeState {
    requires relation MUTATED_BY -> ExecutableComponent
    optional relation AUTHORIZED_BY -> SecurityControl
}
```

## 112.3 Open-world compatibility

Do not force every extension concept to inherit from a central hierarchy.

Interfaces provide structural/semantic compatibility without requiring extensions to surrender namespace ownership.

---

# 113. Named future-state scenarios and transition architectures

Counterfactual overlays should become persistent, named engineering scenarios when users need longer-lived architecture planning.

Model:

```text
Scenario {
    id,
    name,
    base_semantic_generation,
    overlay,
    declarations,
    assumptions,
    status,
    author/provenance,
}
```

Potential status:

```text
DRAFT
PROPOSED
APPROVED
REJECTED
SUPERSEDED
IMPLEMENTED
```

## 113.1 Scenario examples

```text
scenario/auth-v2
scenario/migrate-orders-to-events
scenario/split-monolith-payments
scenario/eu-data-isolation
scenario/remove-legacy-api-v1
```

## 113.2 Scenario comparisons

Support:

```text
current vs scenario A
scenario A vs scenario B
scenario A vs implemented branch
approved scenario vs observed runtime
```

Outputs can include:

```text
semantic changes
contract violations
new/removed dependencies
concept authority changes
data-flow changes
deployment impact
unresolved assumptions
verification predicates
```

## 113.3 Transition architecture

A scenario may contain intermediate states:

```text
CURRENT
  -> TRANSITION_1
  -> TRANSITION_2
  -> TARGET
```

SyntaxMesh models and evaluates those states.

It does **not** execute the migration.

The upper-layer change engine may consume the scenario as a target and plan how to realize it.

---

# 114. Catalog, ownership and organizational evidence

SyntaxMesh should ingest coarse-grained engineering catalogs rather than recreating their UI/product surface.

Generic declared entities:

```text
Domain
System
Component
Service
API
Resource
Team
Owner
Lifecycle
Tier
Repository
DeploymentUnit
```

Generic relations:

```text
BELONGS_TO
OWNS
MAINTAINS
PROVIDES
CONSUMES
DEPENDS_ON
OPERATES
ON_CALL_FOR
```

## 114.1 Declared ownership is evidence, not reality

Keep separate:

```text
DECLARED_OWNER
CODEOWNER
GIT_ACTIVITY_OWNER
ON_CALL_OWNER
RUNTIME_OPERATOR
```

These can disagree legitimately.

Target query:

```text
Who is responsible for this flow?
```

SyntaxMesh may answer:

```text
declared owner: Payments Team
CODEOWNERS: Core Platform
last 90d modifications: 72% Billing Team
on-call: Payments Operations
```

The disagreement itself is valuable.

## 114.2 Catalog drift

Detect:

```text
catalog component has no implementation evidence
repository has no catalog mapping
API provider differs from declaration
runtime service has no declared component
resource ownership is stale
```

Do not automatically rewrite the external catalog.

---

# 115. Runtime dependency evidence summaries

Runtime observations should carry enough quantitative context to distinguish an obscure observed edge from a production-critical path.

Possible summarized evidence:

```text
first_seen
last_seen
observation_count
request_count / event_count
rate
error count/rate
latency quantiles
sample environments
sample deployment versions
trace/span references
source observer
```

SyntaxMesh SHOULD NOT become the primary time-series or trace store.

Keep:

```text
summary + evidence references + bounded samples
```

while external telemetry systems retain raw/high-volume observations.

## 115.1 Environment must be first-class

An edge observed in:

```text
unit_test
local_dev
staging
production
```

has different implications.

Never collapse them.

## 115.2 Runtime criticality

Derived metrics MAY include:

```text
HOT_PATH
FREQUENT_PATH
RARE_PATH
ERROR_DOMINANT_PATH
LATENCY_CRITICAL_PATH
PRODUCTION_ONLY_PATH
TEST_ONLY_PATH
```

These are derived claims with explicit windows/thresholds, not timeless structural truths.

## 115.3 Static/runtime reconciliation

Support findings such as:

```text
static possible + runtime frequent
static possible + runtime never observed
runtime observed + no static explanation
runtime path newly appeared after generation N
```

Again, absence of runtime observation is not proof of impossibility.

---

# 116. Incident, defect, root-cause and decision lineage

Expand the existing rationale/history graph into operational engineering memory.

Potential entities:

```text
Incident
Defect
CustomerIssue
Alert
Symptom
FailureMode
RootCause
ContributingFactor
Mitigation
Workaround
PermanentFix
Regression
Postmortem
Decision
Guardrail
```

Potential relations:

```text
INCIDENT AFFECTED Flow
INCIDENT CAUSED_BY FailureMode
ROOT_CAUSE LOCATED_IN Component
FIX CHANGED Symbol
FIX INTRODUCED Contract
POSTMORTEM MOTIVATED ADR
GUARDRAIL PREVENTS FailureMode
REGRESSION REINTRODUCED FailureMode
```

## 116.1 Why this matters for coding agents

A piece of code that appears redundant may encode a scar from a prior failure.

SyntaxMesh should be able to explain:

```text
validation exists because incident INC-842 exposed malformed settlement messages
retry fence exists because duplicate delivery caused double credit
adapter remains because API v1 consumers still depend on old identity encoding
```

This is far more useful than raw Git blame alone.

## 116.2 Trust classification

Incident systems and tickets are often human-authored and incomplete.

Store them as declared/documented evidence, linked to source/runtime proof where available.

---

# 117. Context packs as first-class measurable artifacts

The context compiler should persist enough metadata to evaluate whether the selected context was actually useful.

Conceptual artifact:

```text
ContextPack {
    id,
    semantic_generation,
    query/intent,
    target,
    selected_facts,
    selected_claims,
    selected_source_spans,
    selected_contracts,
    selected_history,
    omitted_summary,
    token_budget,
    estimated_scores,
    generated_at,
}
```

## 117.1 Outcome feedback

A client MAY later submit outcome evidence:

```text
TaskOutcome {
    context_pack_id,
    client/model identity,
    task success,
    files additionally opened,
    additional searches,
    tests/build result,
    architecture violations introduced,
    contracts violated,
    rework count,
    token usage,
}
```

Do not allow outcome feedback to mutate factual truth.

It tunes ranking/context-selection heuristics only.

## 117.2 Privacy and opt-in

Agent/model outcome data may be sensitive.

Requirements:

```text
local storage by default
explicit external reporting
no source/code upload merely for ranking telemetry
deletable evaluation history
workspace-scoped retention
```

---

# 118. Historical task replay and agent-context evaluation

Build a replay harness over historical engineering tasks.

Inputs may include:

```text
historical base commit
issue/PR/task description
accepted patch/final commit
historical tests
historical semantic generation where available
```

Then evaluate strategies:

```text
agent + grep/read
agent + graph-only context
agent + SyntaxMesh structural context
agent + SyntaxMesh semantic context
agent + alternative ranking strategy
```

Metrics:

```text
task success
semantic equivalence to accepted result where measurable
regressions
contracts violated
tests failed
files opened
files changed unnecessarily
tokens
turns
tool calls
wall-clock
rework after first patch
hallucinated dependency assumptions
missed migration requirements
```

## 118.1 Re-evaluated history must be labeled

Historical replay under today's ontology is different from reproducing what SyntaxMesh knew at the time.

Keep:

```text
HISTORICAL_AS_KNOWN
REPLAYED_WITH_CURRENT_SEMANTICS
```

separate.

## 118.2 Benchmark as product discipline

Do not optimize context ranking by intuition alone.

A context-selection change should ideally demonstrate measurable improvement on a stable task corpus.

---

# 119. Engineering-policy evaluation

Generalize architecture rules/contracts into an evidence-backed engineering-policy layer.

Policy examples:

```text
regulated writes require authorization
public API components require declared ownership
PII may not flow across prohibited region boundaries
new consumers may not depend on deprecated APIs
critical endpoints require threat-model coverage
production-critical flows require at least one integration test
persistent schema changes require migration compatibility evidence
```

## 119.1 Tri-state or richer results

Avoid forcing all policies into boolean pass/fail.

Possible result:

```text
PASS
FAIL
UNKNOWN
INCONCLUSIVE
NOT_APPLICABLE
WAIVED
```

`UNKNOWN` is important when evidence is incomplete.

## 119.2 Waivers / exceptions

Support explicit exceptions:

```text
policy ID
scope
reason
owner
created_at
expires_at
issue/ADR reference
```

Expired waivers become findings.

## 119.3 Policy severity is configuration

The ontology/inference layer establishes facts.

Policy config determines whether a violation:

```text
warns
fails CI
blocks merge
requires approval
```

SyntaxMesh should not hard-code organization-specific severity.

---

# 120. External evidence adapter SDK

Create a generic adapter model for systems that are neither language analyzers nor runtime probes.

Potential extension capability:

```rust
pub trait EvidenceProvider: Send + Sync {
    fn descriptor(&self) -> &EvidenceProviderDescriptor;
    fn evidence_classes(&self) -> &[EvidenceClass];
    fn ingest(&self, request: EvidenceIngestRequest, sink: &mut dyn EvidenceSink)
        -> Result<(), EvidenceProviderError>;
}
```

Potential provider classes:

```text
SOFTWARE_CATALOG
ENTERPRISE_ARCHITECTURE
APM_TRACE_SUMMARY
SAST
INCIDENT_MANAGEMENT
ISSUE_TRACKER
CI_BUILD
TEST_COVERAGE
DEPLOYMENT_SYSTEM
CODEOWNERSHIP
SCHEMA_REGISTRY
PACKAGE_REGISTRY
CUSTOM
```

## 120.1 Provider records remain attributable

Every imported claim/fact must retain:

```text
provider identity
external object identity
external version/revision if available
observed/imported timestamp
trust class
normalization version
source reference
```

## 120.2 Read-oriented integration by default

SyntaxMesh primarily consumes evidence.

Do not make core depend on write access to external systems.

Mutating external systems belongs to specialized clients or the upper-layer change engine.

---

# 121. Evidence reconciliation engine

Multiple providers will produce overlapping or conflicting claims.

Make reconciliation explicit.

Example:

```text
catalog says ServiceA belongs to Payments
repository path says ServiceA lives in shared-core
CODEOWNERS says Platform
runtime says ServiceA primarily talks to Fraud
ontology maps its operations to PaymentAuthorization
```

Do not collapse these into one `team = ...` or `domain = ...` field.

Expose evidence facets and derived reconciliation claims.

Potential relations:

```text
CONFIRMS
REFINES
CONTRADICTS
SUPERSEDES
CORRELATES_WITH
EXPLAINS
```

## 121.1 Query-specific reconciliation

Question:

```text
Who owns deployment responsibility?
```

may weight on-call/deployment evidence.

Question:

```text
Which domain does this code implement?
```

may weight ontology/source/architecture evidence.

The planner should select evidence appropriate to the semantics of the question.

---

# 122. Verification predicates as a first-class export

To support safe upper-layer transformation tooling, SyntaxMesh should emit machine-checkable predicates describing what must hold after a proposed change.

Examples:

```text
ConceptRepresentationExists(CustomerIdentity, protobuf:CustomerRefV2)
NoRepresentationExists(CustomerIdentity, protobuf:LegacyCustomerId)
NoPath(Gateway, Ledger, bypassing=AuthorizationBoundary)
ContractPasses(finance:ledger-write-auth)
NoNewConsumer(api:v1)
AllConsumersCompatible(event:customer-v2)
DataDoesNotFlow(PII.Email, region:prohibited)
TestCoverageAtLeast(flow:purchase, configured_threshold)
```

Conceptual model:

```text
VerificationPredicate {
    id,
    kind,
    inputs,
    semantic_generation_basis,
    evidence_requirements,
    failure_semantics,
}
```

SyntaxMesh evaluates these predicates over a graph/semantic generation.

The upper-layer tool uses them as acceptance criteria.

---

# 123. Change-analysis contract for upper-layer tools

Expose a stable read-only contract that a transformation/planning system can consume.

Conceptual bundle:

```text
ChangeAnalysisBundle {
    intent,
    base_semantic_generation,
    affected_entities,
    affected_concepts,
    change_requirements,
    compatibility_requirements,
    applicable_contracts,
    data_flow_constraints,
    behavior_impacts,
    deployment_impacts,
    relevant_tests,
    historical_evidence,
    known_unknowns,
    contradictions,
    verification_predicates,
}
```

This is not an executable plan.

It is the evidence-backed problem definition.

The external change engine may transform it into:

```text
ordered steps
rewrite operations
migration operations
commands
build/test tasks
commits/PRs
rollback strategy
```

That separation is intentional.

---

# 124. What remains outside SyntaxMesh after v3

Even with the broader evidence model, do not add these responsibilities to SyntaxMesh core:

```text
source-code rewrite execution
AST mutation engine
shell/command execution framework
database migration runner
deployment orchestrator
CI executor
Git hosting / PR automation
APM trace warehouse
log aggregation
incident-management workflow
software-catalog UI
enterprise-architecture diagram editor
LLM coding agent
package manager
build system
```

SyntaxMesh may model or ingest facts from all of them.

That is not the same as becoming them.

---

# 125. v3 roadmap impact

The v3 additions should be staged behind the existing reliable foundation.

## Foundation — unchanged priority

```text
core typed fact model
stable IDs
transactional incremental indexing
provenance
Rust/TypeScript/Python baseline
extension SDK
query engine
Git/worktree model
MCP/context compiler
Penelope-backed durable workflows
StateChronicle optional verified history
DuckDB history/analytics
```

## Provider architecture — reserve early

Before public APIs harden, reserve:

```text
AnalysisProvider identity/capabilities
EvidenceProvider identity/capabilities
fact refinement/confirmation/contradiction links
analysis overlay identity
provider trust/provenance
verification predicate DTOs
```

These are relatively cheap to reserve and expensive to retrofit later.

## v1-era high-value additions

```text
compiler/indexer precision providers
ontology interfaces/capabilities
catalog/ownership evidence
runtime summary evidence
engineering policy results
named scenarios
incident/decision lineage
context-pack identities and evaluation hooks
```

## Later advanced additions

```text
full program-analysis overlays
cross-provider evidence reconciliation heuristics
historical agent replay at scale
learned context ranking
advanced scenario comparison
large-scale security/data lineage providers
```

Implementation pressure and benchmarks decide release assignment.

---

# 126. v3 benchmark expansion

Add provider and reconciliation benchmarks.

Measure:

```text
compiler/provider precision gain over baseline resolver
provider ingest throughput
provider reconciliation latency
incremental invalidation after provider update
storage overhead per evidence class
runtime-summary usefulness vs raw trace dependence
catalog drift precision
incident linkage precision
policy UNKNOWN vs false-positive rate
context-pack agent effectiveness
historical replay reproducibility
```

Important product metric:

```text
How often does additional evidence change an engineering conclusion correctly?
```

Do not reward the system merely for ingesting more facts.

---

# 127. Updated v3 product thesis

SyntaxMesh v3 should now be understood as four cooperating planes:

```text
1. FACT PLANE
   deterministic source/schema/build/runtime/catalog/analyzer evidence

2. KNOWLEDGE PLANE
   concepts, interfaces, identities, declarations, histories

3. REASONING PLANE
   derivations, contracts, policies, drift, scenarios, reconciliation

4. DELIVERY PLANE
   queries, evidence packs, CI results, IDE/MCP/API, verification predicates
```

External systems feed evidence into the first two planes.

Specialized mutation/execution systems consume outputs from the fourth plane.

The core value proposition becomes:

> **SyntaxMesh continuously reconciles what engineering systems say should exist, what source and configuration implement, what specialized analyzers prove, what runtime systems observe, and what engineering history explains—then exposes the resulting knowledge with explicit provenance and reproducible reasoning.**

---

# 128. Updated v3 responsibility summary

```text
SyntaxMesh owns:
    source/system fact extraction
    precision provider ingestion
    external engineering evidence normalization
    stable semantic identity
    graph execution
    ontology and concept interfaces
    deterministic inference
    truth maintenance
    contracts / policies
    intended vs implemented vs observed
    incident/decision linkage
    runtime evidence summaries
    scenarios / counterfactual models
    semantic diff
    change impact + requirements
    verification predicates
    context compilation
    historical analysis
    evidence-backed explanations

SyntaxMesh does NOT own:
    code mutation
    transformation execution
    migration execution
    deployment execution
    agent implementation
    telemetry warehouse
    catalog UI
    incident workflow
```

---

# 129. v3 final architectural progression

```text
source / config / schemas / builds
compiler / LSP / semantic indexes
program-analysis providers
runtime summaries
catalog / ownership
incidents / decisions / ADRs
              |
              v
       attributable evidence
              |
              v
      typed live fact graph
              |
              v
    ontology / interfaces / identity
              |
              v
 derivations / reconciliation / truth
              |
              v
 contracts / policy / drift / scenarios
              |
              v
semantic diff / impact / requirements
              |
              v
 context packs / verification predicates
              |
      +-------+--------+
      |                |
      v                v
 humans / CI      upper change engine
                       |
                       v
             plan -> transform -> verify
```

The upper change engine is intentionally separate and is specified in its own source-of-truth document.

---

# 130. v4 expansion — coding-harness integration as a first-class deployment target

SyntaxMesh is intentionally runtime-agnostic.

That decision should now be elevated into an explicit product invariant:

> **SyntaxMesh MUST be able to operate as the intelligence substrate of an agentic coding harness without requiring the harness to adopt SyntaxMesh's daemon, CLI, MCP transport, Tokio runtime choices, UI, or process topology.**

The open-source OpenAI Codex Rust harness is the first reference integration because it provides a realistic, public, high-quality agent harness on which the value of native SyntaxMesh integration can be measured.

The reference integration is an experiment and adoption vehicle, **not** a change in SyntaxMesh ownership boundaries.

SyntaxMesh must never become:

```text
syntaxmesh-for-codex
```

The architecture remains:

```text
                      SyntaxMesh
          runtime / harness agnostic engine
                           |
             stable application/API boundary
                           |
        +------------------+-------------------+
        |                  |                   |
        v                  v                   v
      Codex             OpenCode            custom harness
      adapter            adapter              adapter
        |                  |                   |
        v                  v                   v
      harness             harness              harness
```

The Codex adapter is one consumer of SyntaxMesh.

No Codex-specific type, prompt shape, session ID, approval model, tool schema, or UI concept may leak into `syntaxmesh-core`, `syntaxmesh-engine`, ontology, inference, storage, provenance, or query semantics.

---

# 131. Current Codex reference architecture — research snapshot 2026-09-23

The public Codex repository should be treated as a moving upstream, not a frozen dependency contract.

At this research point, the important architectural facts are:

```text
openai/codex
    |
    +-- codex-rs/                 Rust Cargo workspace
    |
    +-- codex-core                business logic / agent engine
    |
    +-- protocol / app-server     host and UI protocol surfaces
    |
    +-- tools                     shared host-side tool machinery
    |
    +-- extension API             extension-owned capabilities
    |
    +-- context/world-state       host-owned model-visible state
    |
    +-- sandbox / approvals       controlled execution boundary
    |
    +-- apply_patch               verified patch application machinery
```

The public repository currently documents `codex-core` as business logic used by Codex UIs and ships the Rust implementation as a Cargo workspace.

The repository also contains:

- an extension API
- host-side tool planning/adaptation machinery
- MCP support
- model-visible world/context state
- worktree/repository-aware execution environment concepts
- patch verification and sandbox/approval machinery
- agent extension crates

These surfaces make several integration depths possible without requiring SyntaxMesh to own the harness.

Important warning:

```text
Codex internal modules are NOT SyntaxMesh public APIs.
```

A public fork may integrate against current internals for experimentation.

A long-lived upstreamable adapter should minimize assumptions about private Codex internals and migrate toward explicit extension/provider interfaces whenever upstream exposes suitable stable seams.

---

# 132. Integration objective

The Codex experiment exists to test a concrete hypothesis:

> **A coding agent supplied with a persistent, incremental, evidence-backed model of the repository/system can solve non-trivial engineering tasks with higher first-pass correctness and less repository rediscovery than the same agent using ordinary filesystem/search tools alone.**

The experiment should test a second hypothesis:

> **Native harness integration can outperform an MCP-only integration by allowing SyntaxMesh intelligence to participate automatically in task bootstrap, context maintenance, worktree synchronization, and post-edit verification rather than relying on the model to decide when to invoke a tool.**

The goal is NOT:

```text
make benchmark numbers look good
```

The goal is to establish whether SyntaxMesh produces measurable engineering value.

---

# 133. Four integration depths

Support and benchmark multiple integration depths.

## 133.1 Level 0 — no SyntaxMesh baseline

Upstream-like Codex behavior:

```text
Codex
  -> filesystem
  -> grep/search
  -> shell
  -> apply_patch
  -> compiler/tests
```

This is the mandatory control group.

## 133.2 Level 1 — MCP / tool-only SyntaxMesh

SyntaxMesh runs independently and exposes agent-callable tools.

```text
Codex
   |
   +--> ordinary tools
   |
   +--> MCP
          |
          v
      syntaxmeshd
```

Benefits:

- almost no harness changes
- fastest integration experiment
- works with many harnesses
- useful compatibility baseline

Weakness:

- model must know when to call SyntaxMesh
- semantic context is not automatically injected
- stale-index synchronization may require explicit tool lifecycle
- post-edit semantic verification may be skipped by the model

## 133.3 Level 2 — native sidecar adapter

Codex owns an adapter/client and the SyntaxMesh engine runs in a separate local process.

```text
Codex core
    |
    v
CodexSyntaxMeshAdapter
    |
    v
local SyntaxMesh daemon
```

The harness can automatically:

- open workspace
- refresh index
- request task context
- request impact/contract information
- run verification after mutations

This keeps process isolation while enabling harness-controlled behavior.

## 133.4 Level 3 — direct Rust embedding

The strongest integration path:

```text
Codex process
    |
    +-- codex-core
    |
    +-- codex-syntaxmesh adapter
            |
            v
       syntaxmesh-engine
```

Benefits:

- no IPC overhead for hot queries
- shared process lifecycle
- direct typed API
- easiest automatic synchronization
- lower serialization overhead
- potential reuse of worktree/session lifecycle events

Costs:

- tighter dependency integration
- larger Codex binary/dependency graph
- more careful async/runtime boundaries
- higher risk of fork maintenance if coupled to internals

All three SyntaxMesh integration levels must produce equivalent logical answers for the same semantic generation.

---

# 134. Recommended initial public fork strategy

Do NOT begin with a deeply invasive direct embedding.

Use staged evidence.

Recommended sequence:

```text
Phase A
upstream Codex baseline

Phase B
Codex + SyntaxMesh MCP

Phase C
Codex + native sidecar adapter

Phase D
Codex + direct embedded engine
```

This allows measurement of:

```text
value from SyntaxMesh intelligence
vs
value from tighter integration itself
```

If MCP already captures almost all benefit, invasive embedding may not be justified.

If native integration materially improves context selection, synchronization or verification, the benchmark will demonstrate why.

---

# 135. Public fork repository model

The experimental fork should remain obviously derived from upstream Codex and should preserve upstream history.

Working naming examples:

```text
codex-syntaxmesh
codex-syntaxmesh-experimental
codex-with-syntaxmesh
```

Do not imply OpenAI endorsement.

README language should state clearly:

```text
Experimental public fork of OpenAI Codex integrating SyntaxMesh
as a native repository/system intelligence provider.

Not an official OpenAI project.
```

Keep integration commits isolated where practical.

Suggested branch topology:

```text
upstream/main
    |
    +-- syntaxmesh/integration-base
    |
    +-- syntaxmesh/mcp-benchmark
    |
    +-- syntaxmesh/native-sidecar
    |
    +-- syntaxmesh/native-embedded
```

The fork should regularly rebase or merge upstream rather than becoming a permanent unrelated codebase.

---

# 136. Integration crate boundary

Do not add Codex support to SyntaxMesh core.

Preferred ownership:

```text
Codex fork / adapter repository
    codex-syntaxmesh/
```

or, if upstreamable integration becomes realistic:

```text
codex-rs/syntaxmesh-adapter/
```

The adapter depends on both worlds:

```text
codex APIs / lifecycle
        +
SyntaxMesh public API
        |
        v
codex-syntaxmesh adapter
```

SyntaxMesh itself sees only generic operations:

```text
WorkspaceId
RepositoryId
WorkingTreeIdentity
GraphGeneration
SemanticGeneration
QueryRequest
ContextRequest
VerificationRequest
```

No `CodexSessionId` belongs in the SyntaxMesh public domain model.

---

# 137. Runtime abstraction requirement

Direct Rust embedding must not force SyntaxMesh to adopt Codex's runtime internals.

SyntaxMesh application APIs should remain usable in:

```text
Tokio host
async-std host through adapter if needed
synchronous CLI wrapper
embedded test runtime
standalone daemon
custom executor
```

Avoid hidden global runtime assumptions.

If SyntaxMesh async APIs require Tokio initially, isolate that requirement behind an engine/runtime adapter rather than leaking Tokio handles throughout domain crates.

The core model and deterministic graph/query logic remain runtime-free.

---

# 138. Workspace lifecycle integration

A native harness integration should manage SyntaxMesh as part of the repository lifecycle.

Conceptual lifecycle:

```text
Codex session starts
      |
      v
resolve workspace / git repository
      |
      v
open or create SyntaxMesh workspace
      |
      v
resolve current commit + worktree identity
      |
      v
ensure canonical base generation exists
      |
      v
index working-tree overlay
      |
      v
ready for task context
```

The integration should not re-index the entire repository for every turn.

It should reuse the durable workspace index.

---

# 139. Worktree-aware agent model

Codex and other agent harnesses may use Git worktrees or isolated environments.

SyntaxMesh must treat each worktree as an explicit overlay over shared repository history.

Conceptually:

```text
Repository base graph
        |
        +--> Worktree A overlay -> Agent A
        |
        +--> Worktree B overlay -> Agent B
        |
        +--> Worktree C overlay -> Agent C
```

Requirements:

- [ ] worktree identity must be explicit
- [ ] queries must never accidentally read another agent's working tree
- [ ] immutable source versions should be structurally shared
- [ ] content-addressed extraction results should be reused
- [ ] changed files should produce local overlay facts
- [ ] deleting a worktree must not destroy reusable repository history
- [ ] merging a branch should allow the canonical base generation to advance without duplicating unchanged facts

This is a major expected efficiency advantage for multi-agent coding.

---

# 140. Mutation detection — never trust only `apply_patch`

A harness integration must assume files can change through many mechanisms:

```text
apply_patch
shell command
formatter
compiler code generation
package manager
migration tool
build script
git checkout
external editor
sub-agent
```

Therefore:

```text
"we saw apply_patch"
```

is insufficient as the synchronization protocol.

Use a combination of:

- tool-event hints
- Git/index status
- filesystem watching where reliable
- content hashing
- explicit dirty-path notification
- generation validation before semantic query

The harness may notify SyntaxMesh immediately after a known write, but SyntaxMesh remains responsible for validating content identity before accepting new facts.

---

# 141. Incremental synchronization modes

Support explicit synchronization policies.

```text
EAGER
    re-index affected files after each known mutation batch

LAZY
    mark paths dirty; refresh before next semantic query

TURN_BOUNDARY
    refresh before model receives next turn context

VERIFY_ONLY
    refresh only when verification is requested
```

Recommended native Codex default:

```text
known edits -> mark dirty immediately
before semantic query -> refresh dirty set
before final verification -> force consistency check
```

Do not impose expensive semantic recomputation after every keystroke/patch hunk.

---

# 142. SyntaxMesh native tool surface for Codex

Do not expose hundreds of low-level graph operations to the model.

Start with a small task-oriented tool family.

Suggested model-visible operations:

```text
syntaxmesh_context
syntaxmesh_search
syntaxmesh_impact
syntaxmesh_why
syntaxmesh_contracts
syntaxmesh_flow
syntaxmesh_semantic_diff
syntaxmesh_verify
```

Potential later operations:

```text
syntaxmesh_representations
syntaxmesh_data_lineage
syntaxmesh_drift
syntaxmesh_history
syntaxmesh_scenario
```

Every tool response should include:

```text
graph generation
semantic generation
worktree identity
evidence count
confidence/derivation classes
truncation indication
staleness state
```

The model should never receive a semantic answer detached from its generation identity.

---

# 143. Harness-owned task bootstrap context

The strongest native integration should not wait for the model to call a tool before it receives basic repository intelligence.

At task start:

```text
user request
     |
     v
Codex task bootstrap
     |
     +--> SyntaxMesh intent/context request
     |
     v
bounded TaskContextPack
     |
     v
model
```

A bootstrap pack might contain:

```text
TASK-RELEVANT SYMBOLS
likely target files/symbols

SYSTEM CONCEPTS
relevant domain concepts and representations

ARCHITECTURE
components / boundaries / ownership

CONTRACTS
invariants likely relevant to requested change

TEST / BUILD EVIDENCE
related tests, packages, targets

HISTORY
high-value ADR / incident / previous-change context

UNCERTAINTY
ambiguous mappings or missing evidence
```

Strict token budgets are required.

Do not dump the entire graph into the model.

---

# 144. Two-stage context strategy

Avoid trying to predict the entire task before the agent reasons.

Use:

```text
STAGE 1: bootstrap context
small, high-confidence map

STAGE 2: just-in-time semantic queries
requested as the agent narrows the task
```

This reduces both under-context and over-context.

The harness may automatically request Stage 2 context when certain actions occur, but avoid hidden semantic queries whose results materially affect the task without recording them in the agent trace.

---

# 145. Context pack as a first-class artifact

Every injected context pack should be reproducible.

Conceptual type:

```rust
TaskContextPack {
    id,
    workspace,
    worktree,
    graph_generation,
    semantic_generation,
    task_fingerprint,
    selected_entities,
    selected_claims,
    contracts,
    evidence_refs,
    token_budget,
    token_count,
    ranking_trace,
    omitted_summary,
}
```

This allows later benchmarking:

```text
What did the model know before it made this decision?
```

That question is essential for evaluating agent accuracy.

---

# 146. Integration with Codex world/context state

The Codex reference harness currently maintains host-owned model-visible world/context state and preserves state across context compaction.

The public fork should experiment with representing a small SyntaxMesh state summary as host-owned world state rather than repeatedly injecting identical prose.

Potential persistent state:

```text
SyntaxMesh enabled
workspace ID
worktree identity
current graph generation
current semantic generation
active task-context pack ID
known unresolved semantic warnings
```

Do NOT persist giant graph excerpts into model history.

Detailed evidence should remain queryable by stable IDs.

The adapter must tolerate upstream Codex changing its internal world-state representation.

SyntaxMesh must not depend on that implementation.

---

# 147. Context compaction resilience

When Codex compacts model history, critical engineering constraints must not disappear accidentally.

The adapter should distinguish:

```text
EPHEMERAL CONTEXT
specific source snippets
one-time exploration results

PERSISTENT TASK STATE
active contracts
worktree generation
change target
critical unresolved contradiction
verification obligations
```

After compaction, the harness should be able to reconstruct a concise SyntaxMesh task state from stable identifiers.

This avoids spending context budget on repeating full source evidence.

---

# 148. Agent planning support without stealing planning ownership

SyntaxMesh should not generate the agent's entire implementation plan.

It should provide planning constraints:

```text
affected components
required compatibility boundaries
known migration dependencies
relevant tests
architecture invariants
semantic verification predicates
uncertainties
```

Codex remains responsible for task-level reasoning unless the separate SyntaxMesh Change Engine is explicitly used.

This keeps the experiment scientifically useful:

```text
Does better system knowledge improve the agent?
```

rather than conflating SyntaxMesh with a second planning agent.

---

# 149. Post-edit semantic verification hook

This is one of the highest-value native integration features.

After a meaningful mutation batch:

```text
Codex edits
    |
    v
build/tests as appropriate
    |
    v
SyntaxMesh refreshes changed region
    |
    v
semantic diff
    |
    v
contract / predicate verification
    |
    v
PASS or counterexample
```

A failure should be returned to the agent in actionable form.

Example:

```text
Verification failed:
financial ledger mutation bypasses AuthorizationBoundary.

Counterexample path:
Gateway::refund
 -> LedgerRepository::write
 -> payment_ledger

Expected predicate:
all mutation paths to finance:FinancialLedger
must contain security:AuthorizationBoundary.
```

This gives the model compiler-like feedback for architecture/semantic mistakes.

---

# 150. Verification modes

Support at least:

```text
STRUCTURAL
call/import/dependency expectations

ARCHITECTURAL
layer and boundary constraints

SEMANTIC
ontology/contract predicates

DATA_FLOW
selected lineage / taint predicates

COMPATIBILITY
API/schema/event compatibility claims

DRIFT
intended vs implemented comparison
```

Verification must return:

```text
PASS
FAIL
UNKNOWN
```

Never collapse lack of evidence into PASS.

---

# 151. Fail-open vs fail-closed behavior

SyntaxMesh availability must not brick ordinary Codex usage by default.

Default integration semantics:

```text
ordinary coding assistance:
    SyntaxMesh unavailable -> degrade gracefully

explicit semantic verification requested:
    SyntaxMesh unavailable -> UNKNOWN / verification unavailable

policy configured as mandatory:
    failed verification -> fail closed
```

Examples:

```text
context enrichment can fail open
security invariant verification can be configured fail closed
```

Make the difference explicit in configuration.

---

# 152. Configuration model

The Codex fork should expose a narrow integration configuration.

Illustrative only:

```toml
[syntaxmesh]
enabled = true
mode = "embedded"          # mcp | sidecar | embedded
sync = "lazy"
bootstrap_context = true
post_edit_verify = true
fail_open_context = true

[syntaxmesh.context]
max_tokens = 5000
include_history = true
include_contracts = true
include_runtime = false

[syntaxmesh.verification]
architecture = true
semantic = true
data_flow = false
```

Do not copy this configuration into SyntaxMesh's core domain model.

The adapter translates host configuration into SyntaxMesh API calls.

---

# 153. Feature flag / optional dependency strategy

A public Codex fork should make SyntaxMesh removable.

Potential Rust build shape:

```toml
[features]
default = []
syntaxmesh = ["dep:syntaxmesh-engine", "dep:codex-syntaxmesh"]
```

or keep it as a separate binary/package variant if upstream build complexity makes workspace features undesirable.

The experiment should be able to build:

```text
upstream-compatible Codex
Codex + SyntaxMesh
```

from closely related source states.

This improves benchmark validity.

---

# 154. Licensing boundary

Current research baseline:

```text
OpenAI Codex repository: Apache-2.0
SyntaxMesh planned license: MIT
```

This combination is generally suitable for a public integration fork.

Requirements:

- preserve Codex Apache-2.0 license and notices
- preserve SyntaxMesh MIT license
- clearly identify modified fork behavior
- do not imply OpenAI sponsorship or endorsement
- retain third-party dependency notices
- perform an actual release-license audit before distributing binaries

SyntaxMesh should remain independently MIT-licensed rather than changing license specifically for Codex.

---

# 155. Security boundary

Embedding SyntaxMesh must not weaken Codex sandbox/approval semantics.

SyntaxMesh is primarily a read/intelligence engine.

It should not receive arbitrary extra filesystem privileges merely because it runs in-process.

Requirements:

- [ ] repository reads respect the host workspace boundary
- [ ] external analyzers run under explicit permissions
- [ ] no hidden network access
- [ ] semantic providers declare network requirements
- [ ] untrusted repository content remains data, never control instructions
- [ ] external runtime/catalog integrations remain explicitly configured
- [ ] no SyntaxMesh query may silently perform code mutation

If the separate Change Engine is later integrated, its mutation permissions remain subject to Codex's own sandbox and approval machinery.

---

# 156. Local-first privacy advantage

A native SyntaxMesh integration should preserve one of its strongest properties:

```text
repository intelligence can remain local
```

The local engine can compute:

- symbol graph
- impact
- contracts
- semantic mappings
- context ranking
- verification predicates

without uploading the entire repository to a separate hosted graph service.

The model may still receive selected source/context according to the user's Codex configuration, but SyntaxMesh itself must not introduce additional source exfiltration.

---

# 157. Index bootstrap strategy

First-run indexing latency matters enormously for harness adoption.

The Codex experiment should distinguish:

```text
COLD START
no SyntaxMesh state exists

WARM START
persistent repository index exists

HOT START
same worktree/generation already active
```

Measure all three.

Potential UX:

```text
first task begins immediately with ordinary Codex tools
        |
        +--> SyntaxMesh builds baseline incrementally
        |
        v
semantic features become available when ready
```

Do not necessarily block a trivial task on indexing an enormous monorepo.

For tasks requiring explicit semantic verification, wait for the required graph region rather than the whole repository where possible.

---

# 158. Priority indexing for agent tasks

Full repository indexing is useful but task latency can improve by prioritizing likely-relevant regions.

Potential strategy:

```text
1. discover repo/package metadata
2. identify likely task targets lexically
3. prioritize target files/packages
4. build surrounding semantic neighborhood
5. continue background/full indexing through normal engine workflow
```

Important:

Do not let task-priority indexing create a permanently partial graph without an explicit completeness state.

Every query should know whether evidence is:

```text
COMPLETE_FOR_SCOPE
PARTIAL
STALE
```

---

# 159. Tool-result density optimization

Native integration should optimize information per model token.

A semantic query result should prefer:

```text
claim
why it matters
exact evidence references
critical counterexample path
```

instead of:

```text
hundreds of raw nodes and edges
```

The model can drill down if necessary.

This makes SyntaxMesh useful even when the underlying graph is very large.

---

# 160. Evidence reference protocol

Every model-visible semantic claim should expose stable evidence handles.

Example:

```text
claim_id: claim_...
evidence:
  - src://repo/path.rs#L42-L67
  - fact://...
  - derivation://...
```

The Codex adapter can translate handles into model-friendly source retrieval actions.

This avoids injecting all supporting source eagerly.

---

# 161. Integration with ordinary Codex tools

SyntaxMesh does not replace:

```text
shell
rg
git
apply_patch
compiler
tests
```

It changes when and why they are used.

Desired agent pattern:

```text
SyntaxMesh tells agent where/why
        |
        v
ordinary Codex tools inspect exact implementation
        |
        v
agent edits
        |
        v
ordinary compiler/tests + SyntaxMesh verification
```

This preserves the strengths of the existing harness.

---

# 162. Avoid forcing every filesystem read through SyntaxMesh

Do not create a proprietary virtual filesystem abstraction merely to ensure SyntaxMesh sees reads.

The agent should still be able to inspect source normally.

SyntaxMesh's value is persistent system intelligence, not mediation of every byte read.

Likewise, do not route ordinary shell execution through SyntaxMesh.

---

# 163. Multi-agent shared intelligence

The Codex fork should explicitly benchmark parallel-agent scenarios.

Potential topology:

```text
                   Repository base
                         |
                  SyntaxMesh store
                         |
       +-----------------+-----------------+
       |                 |                 |
       v                 v                 v
   worktree A        worktree B        worktree C
       |                 |                 |
     agent A           agent B           agent C
```

Reuse:

- content hashes
- parsed representations
- immutable graph facts
- ontology
- package metadata
- historical facts

Isolate:

- working-tree overlays
- task context
- uncommitted mutations
- verification results

The benchmark should measure whether shared intelligence reduces duplicate reads/tool calls across agents.

---

# 164. Cross-agent conflict intelligence

Later, SyntaxMesh may expose potential semantic overlap between active worktrees:

```text
Agent A modifies representation of CustomerIdentity
Agent B modifies API consuming CustomerIdentity
```

Potential advisory finding:

```text
semantic overlap detected
```

This is not a merge-conflict engine.

It is early warning based on shared conceptual dependencies.

Do not block agents automatically without measured evidence that the policy is useful.

---

# 165. Codex + SyntaxMesh benchmark matrix

A convincing public result requires controlled comparisons.

At minimum:

```text
A. upstream Codex baseline
B. Codex + SyntaxMesh MCP
C. Codex + SyntaxMesh native sidecar
D. Codex + SyntaxMesh embedded
```

Keep fixed where possible:

```text
model
reasoning effort
system prompt
repository state
sandbox policy
task statement
compiler/test environment
maximum wall-clock / token budget
```

Because models are nondeterministic, repeat tasks across multiple runs/seeds where the harness supports meaningful variation.

Report distributions, not cherry-picked single runs.

---

# 166. Task corpus

Build tasks from several classes.

## 166.1 Small/local

```text
fix one function
add isolated validation
rename local symbol
```

Expected SyntaxMesh benefit: low.

## 166.2 Repository-scale

```text
change public API
modify shared type
fix cross-module bug
refactor subsystem boundary
```

Expected benefit: moderate to high.

## 166.3 Monorepo/cross-package

```text
shared schema migration
SDK/API update
build-target impact
large refactor
```

Expected benefit: high.

## 166.4 Multi-repository/system

```text
API producer + consumers
protobuf/event migration
service boundary move
cross-repo concept change
```

Expected benefit: potentially very high.

## 166.5 Architecture/security-sensitive

```text
authorization boundary
PII lineage
privileged state mutation
forbidden dependency
```

Expected benefit: correctness rather than only token reduction.

## 166.6 Historical/rationale

```text
modify weird compatibility adapter safely
remove deprecated path
change behavior with incident history
```

Tests whether rationale/incident knowledge prevents destructive cleanup.

---

# 167. Core agent metrics

Measure:

```text
task success
first-pass success
human acceptance
wall-clock
input tokens
output tokens
tool calls
source files opened
search commands
patch attempts
files modified
unnecessary files modified
test iterations
compiler failures
rollback/rework count
```

Do not optimize token count at the expense of correctness.

---

# 168. SyntaxMesh-specific correctness metrics

Measure additionally:

```text
hallucinated dependency claims
missed affected consumers
missed tests
architecture violations introduced
semantic contract violations introduced
verification UNKNOWN rate
false-positive verification failures
false-negative verification passes
context precision
context recall
relevant evidence omitted
irrelevant context injected
```

The strongest success result is not:

```text
fewer tokens
```

but:

```text
higher correctness with equal or lower total agent cost
```

---

# 169. Context ablation studies

Do not benchmark only "all SyntaxMesh" vs baseline.

Run ablations:

```text
structural graph only
+ provenance
+ contracts
+ ontology/concepts
+ history/rationale
+ runtime evidence
+ post-edit verification
```

This shows which capabilities actually improve Codex.

It also prevents speculative features from surviving merely because the total system works.

---

# 170. Native integration ablation studies

Separate intelligence from integration mechanics.

Compare:

```text
same SyntaxMesh context through MCP
vs
same context through native bootstrap injection
```

and:

```text
model manually calls verification
vs
harness automatically runs verification
```

This proves whether native embedding itself is valuable.

---

# 171. Repository rediscovery metric

Introduce an explicit metric:

```text
RediscoveryCost
```

Possible components:

```text
search commands
files opened before first edit
repeated reads of previously indexed facts
model tokens spent on architecture reconstruction
```

The central SyntaxMesh hypothesis predicts a large reduction in rediscovery cost on non-trivial repositories.

---

# 172. Context efficiency metric

Measure:

```text
useful engineering evidence / input token
```

Possible proxy:

```text
ContextEfficiency =
relevant evidence items used by successful solution
/
context tokens supplied
```

Do not overfit this into one universal number, but track it across task classes.

---

# 173. Verification value metric

Track how often SyntaxMesh post-edit verification catches defects not caught by the normal immediate test/build loop.

Classify catches:

```text
architecture
compatibility
semantic invariant
data flow
cross-repo impact
stale assumption
```

This may become more important than token savings.

---

# 174. Model-tier experiment

A major economic experiment:

```text
strong model + baseline Codex
strong model + SyntaxMesh
smaller/faster model + SyntaxMesh
```

Question:

> Can high-quality structured engineering context let a cheaper model solve tasks that otherwise require a larger model?

Do not assume the answer.

Benchmark it.

If true, this becomes a major adoption argument for harness vendors.

---

# 175. Cold-start vs amortized economics

SyntaxMesh adds indexing cost.

Measure total economics over:

```text
one task
10 tasks
100 tasks
long-lived developer workspace
parallel agent fleet
```

Expected pattern:

```text
cold first task:
SyntaxMesh may cost more

long-lived workspace:
index cost amortizes

multi-agent workspace:
shared intelligence may amortize strongly
```

Publish both sides honestly.

---

# 176. Benchmark reproducibility artifact

Every benchmark run should record:

```text
Codex commit
SyntaxMesh commit
integration commit
model ID/revision
reasoning configuration
repository commit
worktree state
SyntaxMesh graph generation
SyntaxMesh semantic generation
context-pack ID
prompt/task
sandbox configuration
available tools
result patch
compiler/test results
verification report
usage/timing metrics
```

This makes public claims auditable.

---

# 177. No benchmark-specific hidden intelligence

Do not manually encode task answers into SyntaxMesh declarations for benchmark repositories.

Allowed:

- real architecture declarations already part of repo
- generic extractors
- generic rules
- generic ontology

Disallowed:

```text
special-case knowledge added because benchmark task is known
```

Otherwise the evaluation becomes meaningless.

---

# 178. Upstream-friendly design principle

The public fork should be designed so that a successful experiment can be proposed upstream as one of these forms:

```text
A. optional SyntaxMesh adapter
B. generic repository-intelligence provider interface
C. generic context-provider extension API
D. generic semantic verification hook
```

The best upstream contribution may NOT be:

```text
OpenAI Codex must depend on SyntaxMesh
```

A more maintainable upstream outcome may be a generic interface through which SyntaxMesh becomes one provider.

This also benefits other repository-intelligence systems.

---

# 179. Preferred upstream abstraction

If benchmark results justify it, propose a generic interface approximately like:

```rust
trait RepositoryIntelligenceProvider {
    async fn open_workspace(...);
    async fn task_context(...);
    async fn query(...);
    async fn notify_dirty(...);
    async fn verify(...);
}
```

Exact API must follow upstream Codex conventions at implementation time.

SyntaxMesh then implements it.

This produces:

```text
Codex
   |
   v
RepositoryIntelligenceProvider
   |
   +--> SyntaxMesh
   +--> future provider
   +--> enterprise provider
```

This is more upstreamable than hard-coding SyntaxMesh concepts throughout `codex-core`.

---

# 180. Upstream proposal evidence package

Do not approach upstream primarily with architectural claims.

A serious proposal should contain:

```text
1. minimal integration design
2. benchmark methodology
3. reproducible public task corpus
4. measured correctness improvement
5. measured token/tool reduction
6. performance/indexing cost
7. memory/disk overhead
8. failure/degradation behavior
9. security analysis
10. maintenance burden
11. clean optionality story
```

The evidence should make the integration useful even to reviewers who do not care about SyntaxMesh as a project.

---

# 181. Minimal-diff fork discipline

To maximize upstreamability:

- avoid unrelated Codex changes
- do not restyle/reformat upstream files unnecessarily
- keep adapter logic in dedicated modules/crates
- prefer extension/provider hooks over invasive orchestration rewrites
- keep prompts generic where possible
- add tests alongside each integration seam
- document every place where fork behavior intentionally diverges

A fork that modifies half of `codex-core` will be difficult to maintain and difficult to upstream regardless of benchmark quality.

---

# 182. Codex-specific logic belongs in adapter tests

SyntaxMesh conformance tests should not depend on Codex.

Create separate integration tests:

```text
codex-syntaxmesh tests
```

for:

- workspace open/close
- worktree switch
- dirty-file notification
- context bootstrap
- context compaction restoration
- semantic tool calls
- post-edit verification
- SyntaxMesh unavailable
- index stale
- daemon crash/restart
- embedded engine panic isolation where practical

This preserves independent SyntaxMesh correctness.

---

# 183. Compatibility with upstream Codex releases

The fork should continuously test against upstream main or selected release commits.

Potential CI:

```text
fetch upstream
merge/rebase integration branch
build
run adapter tests
run representative agent smoke tests
```

Track:

```text
fork delta size
merge conflicts per upstream release
adapter API break frequency
```

If maintenance cost becomes large, that is evidence the integration is coupled at the wrong layer.

---

# 184. Version compatibility matrix

Document tested combinations:

```text
Codex commit/version
SyntaxMesh version
adapter version
integration mode
```

Never imply arbitrary compatibility with all future Codex versions.

The public fork can pin exact versions for reproducible benchmarks.

---

# 185. Telemetry for the experiment

Collect local benchmark telemetry only when explicitly enabled.

Useful integration telemetry:

```text
SyntaxMesh query latency
refresh latency
context-pack size
cache hit rate
files avoided
verification latency
index CPU/memory/disk
```

Do not upload proprietary repository facts by default.

Public benchmark repositories can enable richer tracing.

---

# 186. Performance budgets

Native integration is a failure if every turn becomes slow.

Set budgets by latency class.

Illustrative targets to benchmark, not promises:

```text
HOT
symbol / contract lookup
~interactive latency

WARM
small impact/context refresh
low hundreds of milliseconds where possible

COLD
full index / large semantic diff
allowed to be asynchronous or explicitly awaited
```

Do not put large ontology rebuilds in the normal model-turn critical path.

---

# 187. Memory/disk budgets

A coding harness must run on developer machines.

Measure:

```text
index DB size / source size
RAM per million nodes/edges
additional RAM when embedded in Codex
worktree overlay cost
semantic materialization cost
```

Direct embedding should not duplicate huge in-memory graph structures unnecessarily.

Use mapped/compact projections or on-demand materialization where benchmarks justify them.

---

# 188. Crash and corruption behavior

A corrupted or unavailable SyntaxMesh index must not corrupt the source repository.

Required behavior:

```text
index corruption
    -> quarantine/rebuild index
    -> source untouched

semantic layer failure
    -> structural graph remains usable

adapter crash
    -> Codex can degrade or restart integration
```

In embedded mode, isolate recoverable SyntaxMesh errors through `Result`, not process aborts.

Fuzz adapter deserialization and provider payloads.

---

# 189. Harness restart behavior

Persistent SyntaxMesh state should survive Codex process restart.

On restart:

```text
open workspace
validate repo/worktree identity
validate current content/git state
reuse compatible graph generation
incrementally refresh dirty paths
```

Do not assume process lifetime equals repository-intelligence lifetime.

This is important for amortizing indexing cost.

---

# 190. Semantic state in long-running tasks

A long agent run may change the repository substantially.

The adapter must distinguish:

```text
context computed at generation 10
current worktree at generation 17
```

Before relying on old semantic claims, verify whether their premises remain valid.

SyntaxMesh truth-maintenance/invalidation should make this cheap.

The model should receive a stale-context warning when material assumptions changed.

---

# 191. Stale-plan / stale-context detection

Even without the separate Change Engine, Codex may form plans based on previous context.

The adapter can return:

```text
TaskContextPack P was based on semantic generation 104.
Current generation is 109.
3 premises used by P changed.
```

The agent can then refresh before continuing.

Do not silently pretend old context is current.

---

# 192. Human-visible diagnostics

A public fork should let users understand what SyntaxMesh is doing.

Potential commands/status:

```text
/syntaxmesh status
/syntaxmesh context
/syntaxmesh generation
/syntaxmesh verify
```

or equivalent TUI surfaces.

Do not overwhelm ordinary users with graph internals.

Useful status:

```text
indexed
refreshing
partial
stale
verification failed
unavailable
```

---

# 193. User control

Users must be able to disable SyntaxMesh entirely.

They should also be able to disable individual behaviors:

```text
automatic context injection
automatic verification
history ingestion
runtime evidence
external analyzer providers
networked semantic providers
```

This is important for privacy, performance and benchmark validity.

---

# 194. Public-fork README benchmark section

The fork README should include a transparent comparison table once real results exist.

Do NOT publish invented numbers.

Template:

```text
Task set: X
Model: Y
Codex commit: Z
SyntaxMesh commit: S

Metric                 Baseline   MCP   Native
------------------------------------------------
Task success
First-pass success
Input tokens
Tool calls
Files opened
Patch attempts
Wall-clock
Semantic violations
```

Link raw benchmark artifacts.

---

# 195. No promise of OpenAI adoption

The project may hope that upstream Codex maintainers find the integration useful.

Do not position that as an expected outcome.

Correct framing:

```text
Build a useful public experiment.
Prove the value independently.
Keep it upstreamable.
If OpenAI or another harness adopts the interface or integration, that is a consequence of demonstrated value.
```

The fork must remain worthwhile even if it is never upstreamed.

---

# 196. Broader harness portability

Every lesson from the Codex experiment should be generalized back into the SyntaxMesh integration contract.

Avoid Codex-only abstractions such as:

```text
CodexTaskContext
```

Prefer:

```text
HarnessTaskContext
WorkspaceBinding
MutationNotice
VerificationRequest
```

Then implement:

```text
Codex adapter
OpenCode adapter
other harness adapter
```

The Codex fork proves the pattern; it must not become the pattern itself.

---

# 197. Generic harness adapter SDK

Later create a small optional integration crate or protocol:

```text
syntaxmesh-harness-sdk
```

Potential responsibilities:

```text
workspace lifecycle DTOs
worktree identity
mutation notices
context requests
verification requests
context-pack DTOs
staleness notifications
```

It should NOT contain:

```text
LLM API
agent orchestration
prompt templates
shell execution
patch execution
```

Those belong to the harness.

---

# 198. Harness integration conformance suite

A generic adapter should pass tests such as:

```text
opens repository
creates/reuses index
switches worktree without leakage
marks file dirty
refreshes before query
survives process restart
reports stale generation
returns bounded context
returns exact evidence
verifies changed state
handles unavailable engine
```

This will make future harness integrations substantially easier.

---

# 199. Relationship to SyntaxMesh Change Engine

Do not bundle the Change Engine into the first Codex/SyntaxMesh experiment.

First prove:

```text
Codex + SyntaxMesh intelligence
```

Then optionally test:

```text
Codex + SyntaxMesh + Change Engine
```

The layers remain:

```text
Codex
= agent reasoning / implementation

SyntaxMesh
= world model / evidence / constraints / verification

Change Engine
= optional deterministic change planning and transformation orchestration
```

This allows attribution of performance gains.

---

# 200. Later Change Engine integration mode

If added later:

```text
user intent
    |
    v
Codex / agent understanding
    |
    v
SyntaxMesh ChangeRequirements
    |
    v
Change Engine plan DAG
    |
    v
Codex / codemod / transformer executes permitted steps
    |
    v
SyntaxMesh re-index + verification
```

Codex may act as one transformation provider.

It should not be the sole verifier of its own changes.

---

# 201. Native semantic preconditions for tools

A future experiment may let SyntaxMesh attach optional semantic preconditions to risky actions.

Example:

```text
before deleting public API type:
ensure no supported consumers remain
```

Do not initially block ordinary `apply_patch` based on semantic rules.

Start advisory.

Only move to enforceable preconditions after false-positive rates are measured and policy is explicitly configured.

---

# 202. Semantic postconditions for agent completion

A task may carry explicit postconditions:

```text
legacy auth has no remaining production callers
API v1 remains compatible
all ledger writes still pass authorization
```

Before Codex finalizes the task, the harness may automatically run SyntaxMesh verification.

If result is `UNKNOWN`, report uncertainty rather than fabricating success.

---

# 203. Definition of "more deterministic"

The Codex integration documentation must avoid claiming that SyntaxMesh makes the LLM deterministic.

Correct model:

```text
LLM reasoning remains probabilistic

but

system state
context selection
constraint evaluation
verification
provenance
change generations

can become more explicit, reproducible and deterministic
```

The engineering workflow becomes more deterministic even when the model does not.

---

# 204. Determinism envelope

Define the deterministic envelope:

```text
source hash
extractor version
resolver version
ontology version
rule version
configuration fingerprint
worktree identity
        |
        v
structural facts
        |
        v
deterministic claims
        |
        v
verification predicates
```

Model-generated suggestions remain outside this envelope unless promoted through explicit evidence/confirmation.

---

# 205. Agent trace + SyntaxMesh evidence trace

For research runs, correlate:

```text
agent decision
        |
        +--> context pack ID
        +--> semantic query IDs
        +--> evidence refs
        +--> graph/semantic generation
```

This allows post-hoc analysis of whether an incorrect agent decision occurred because:

```text
SyntaxMesh evidence was wrong
SyntaxMesh omitted relevant evidence
agent ignored correct evidence
agent misinterpreted evidence
repository lacked enough evidence
```

That separation is scientifically valuable.

---

# 206. Error taxonomy for benchmark failures

Classify failures instead of reporting only task success.

```text
DISCOVERY_FAILURE
    relevant entity never found

CONTEXT_FAILURE
    found but omitted from model context

SEMANTIC_FAILURE
    wrong concept/relationship inferred

AGENT_REASONING_FAILURE
    correct evidence supplied, wrong decision

TRANSFORMATION_FAILURE
    plan understood, implementation wrong

VERIFICATION_FAILURE
    defect not detected

HARNESS_FAILURE
    timeout/crash/tool issue
```

This tells you where SyntaxMesh actually helps.

---

# 207. Upstream neutrality

If generic Codex provider hooks are proposed upstream, design them so OpenAI does not need to endorse SyntaxMesh-specific semantics.

For example, upstream might only know:

```text
context provider
repository intelligence provider
verification provider
```

SyntaxMesh-specific concepts remain behind the adapter.

This is a cleaner contribution boundary.

---

# 208. Public API stability implications for SyntaxMesh

A real harness integration will pressure-test SyntaxMesh APIs.

Before declaring stable harness APIs:

- run Codex integration through several upstream updates
- identify operations repeatedly needed by the harness
- separate accidental implementation details from durable concepts
- stabilize only the narrow interfaces proven by use

Do not freeze the entire engine API prematurely just because the first fork needs it.

---

# 209. Expected high-value SyntaxMesh APIs for harnesses

Likely durable operations:

```text
open_workspace
bind_worktree
mark_dirty
refresh
status
search
context
impact
why
contracts
semantic_diff
verify
```

These are stronger candidates for stable APIs than storage/internal graph projection methods.

---

# 210. Reference integration data flow

```text
USER TASK
   |
   v
CODEX HARNESS
   |
   +---- resolve workspace/worktree -------------------+
   |                                                   |
   |                                                   v
   |                                            SyntaxMesh Engine
   |                                                   |
   +---- request bootstrap context ------------------->|
   |                                                   |
   |<--- evidence-backed TaskContextPack --------------+
   |
   v
MODEL REASONS
   |
   +---- semantic queries as needed -------------------> SyntaxMesh
   |
   +---- source/shell/git/apply_patch -----------------> Codex tools
   |
   v
MUTATED WORKTREE
   |
   +---- dirty paths / generation refresh ------------> SyntaxMesh
   |
   +---- compiler/tests -------------------------------> Codex tools
   |
   +---- semantic verification -----------------------> SyntaxMesh
   |
   +<--- pass/fail/unknown + counterexamples ----------+
   |
   v
MODEL REPAIRS OR COMPLETES
```

---

# 211. Reference public fork crate layout

Illustrative only; adapt to current upstream workspace at implementation time.

```text
codex-rs/
    core/
    tools/
    ... upstream crates ...

    syntaxmesh-adapter/
        src/
            lib.rs
            config.rs
            workspace.rs
            context.rs
            tools.rs
            verification.rs
            status.rs
        tests/
```

If direct dependency inside upstream workspace causes excessive churn, keep the adapter in a sibling workspace or external crate and integrate through a narrower host extension seam.

---

# 212. Sidecar protocol experiment

Before direct embedding, implement a stable sidecar protocol using SyntaxMesh public DTOs.

Potential operations:

```text
HELLO / capabilities
OPEN_WORKSPACE
BIND_WORKTREE
MARK_DIRTY
REFRESH
CONTEXT
QUERY
VERIFY
STATUS
SHUTDOWN
```

Transport may be:

```text
framed local IPC
stdio
Unix socket / named pipe
localhost HTTP
```

Do not invent a complex distributed protocol for this experiment.

---

# 213. Direct-embedding experiment

After the sidecar establishes semantics, replace transport with direct calls while keeping logical operations unchanged.

This gives an apples-to-apples comparison:

```text
same SyntaxMesh semantics
same context pack
same model
same task
only integration transport/lifecycle differs
```

That is the correct way to measure native embedding benefit.

---

# 214. Avoid hidden prompt dependence

Do not require a giant custom Codex system prompt explaining SyntaxMesh.

The integration should expose intuitive tool descriptions and concise host context.

A good result should come primarily from better information architecture, not a benchmark-specific prompt full of instructions like:

```text
always call SyntaxMesh before every action
```

Native integration is specifically intended to remove that dependence.

---

# 215. Agent autonomy remains intact

SyntaxMesh should not force the agent down one implementation route unless an explicit policy/contract requires it.

It provides:

```text
facts
constraints
evidence
counterexamples
```

The model still chooses implementation strategy.

This preserves creative problem solving while reducing factual uncertainty.

---

# 216. Verification independent from model output

Never verify by asking the same model:

```text
"Does your patch look correct?"
```

SyntaxMesh verification must re-read/re-index actual changed repository state.

Compiler/tests/analyzers must run independently.

The agent's prose explanation is not evidence of correctness.

---

# 217. Benchmark on repositories SyntaxMesh did not design

Do not evaluate only STEXS repositories.

Include:

- mature Rust repositories
- TypeScript services
- Python repositories
- mixed-language monorepos
- external open-source systems with known architecture

Otherwise the ontology/extractor design may overfit STEXS conventions.

---

# 218. Public benchmark credibility

For public claims:

- publish task definitions
- publish repository commits
- publish harness commits
- publish scoring criteria
- publish failed runs
- disclose excluded runs and reasons
- avoid hand-picking only favorable tasks

This matters if the long-term goal includes convincing upstream maintainers or other harness vendors.

---

# 219. Adoption path if results are strong

Plausible progression:

```text
1. SyntaxMesh standalone OSS
2. SyntaxMesh MCP integration works with Codex
3. public Codex fork demonstrates native benefit
4. generic harness adapter SDK stabilizes
5. other harnesses integrate
6. upstream Codex discussion/PR for generic provider seam or optional integration
7. independent ecosystem adoption regardless of upstream decision
```

Do not reverse this order by prematurely optimizing for upstream acceptance.

---

# 220. v4 roadmap placement

The Codex experiment should not block SyntaxMesh v0.1.

Recommended sequencing:

## SyntaxMesh v0.1

Ship reliable standalone foundation:

```text
incremental graph
provenance
core language support
query engine
MCP
context compiler
Git/worktree identity
```

## Early integration experiment

Once those primitives are stable enough:

```text
Codex + SyntaxMesh MCP baseline
```

## Pre-v1 integration hardening

Add:

```text
harness SDK
native sidecar adapter
worktree overlays
context-pack identity
semantic verification API
```

## Later

Test:

```text
direct embedded engine
multi-agent shared store
ontology/contracts
semantic diff
Change Engine integration
```

The experiment should evolve with the actual capabilities implemented, not require the full v3/v4 vision on day one.

---

# 221. v4 success criteria

The Codex reference integration is successful if it demonstrates at least one of these robustly across non-trivial tasks:

```text
higher task success at comparable cost
lower token/tool cost at comparable success
fewer first-pass regressions
fewer architecture/semantic violations
less repository rediscovery
better multi-agent scaling
smaller model reaches comparable result quality
```

The strongest outcome would demonstrate several simultaneously.

---

# 222. v4 failure criteria

Be willing to reject assumptions.

The native integration is not justified if measurements show:

```text
MCP performs equivalently with much lower complexity
indexing overhead exceeds context savings
semantic context adds noise and hurts agents
verification false positives cause excessive rework
fork maintenance cost is unreasonable
small/local tasks dominate real usage and gain little
```

In that case, keep SyntaxMesh standalone/MCP and improve only the parts that measurements support.

---

# 223. v4 architectural invariant — no privileged Codex path

The Codex adapter must use the same public SyntaxMesh capabilities available to other harnesses.

Do not create:

```text
syntaxmesh_internal_for_codex
```

with privileged semantic operations unavailable to the public SDK.

Codex may receive optimized in-process transport, but logical semantics remain public and reusable.

This mirrors the existing extension philosophy for private STEXS integrations.

---

# 224. v4 updated product positioning

SyntaxMesh remains:

> **a local-first, incremental, provenance-backed source and system intelligence engine that turns repositories and engineering evidence into a continuously maintained engineering world model.**

v4 adds a second positioning statement for agent harnesses:

> **SyntaxMesh can serve as the persistent engineering memory, evidence layer, and semantic verifier beneath coding agents—reducing how much architecture they must rediscover from raw files on every task.**

Do not position it as:

```text
an AI model
an autonomous coding agent
Codex replacement
a prompt framework
```

---

# 225. v4 updated responsibility model

```text
CODING HARNESS / CODEX
    task/session lifecycle
    model interaction
    shell/tools
    sandbox/approvals
    patch application
    agent orchestration

SYNTAXMESH
    persistent repository/system understanding
    graph + ontology + evidence
    context selection
    impact/contracts/history
    semantic verification

MODEL
    reasoning
    design choices
    implementation generation

COMPILER / TEST / ANALYZERS
    executable/static correctness evidence

OPTIONAL CHANGE ENGINE
    durable change planning / transformation orchestration
```

No layer should silently absorb another's responsibility.

---

# 226. v4 reference architecture

```text
                         USER
                          |
                          v
                  +----------------+
                  | Codex Harness  |
                  +--------+-------+
                           |
          +----------------+----------------+
          |                |                |
          v                v                v
       Model           Codex Tools      SyntaxMesh Adapter
                           |                |
                    shell/git/patch         |
                                            v
                                  +-------------------+
                                  | SyntaxMesh Engine |
                                  +---------+---------+
                                            |
                +---------------------------+---------------------------+
                |                           |                           |
                v                           v                           v
          canonical facts             semantic model               history
       source/schema/build        ontology/contracts/truth      git/ADR/incidents
                |                           |                           |
                +---------------------------+---------------------------+
                                            |
                                            v
                              evidence-backed context / verify
                                            |
                                            v
                                      Codex Harness
```

---

# 227. v4 final architectural progression

```text
SyntaxMesh standalone foundation
        |
        v
MCP integration with coding agents
        |
        v
reference Codex fork
        |
        +--> native lifecycle integration
        +--> worktree-aware shared intelligence
        +--> context-pack bootstrap
        +--> semantic tools
        +--> post-edit verification
        |
        v
controlled benchmark suite
        |
        +--> measure correctness
        +--> measure context/tool efficiency
        +--> measure indexing cost
        +--> measure multi-agent value
        |
        v
prove or reject native-integration hypothesis
        |
        +--> if weak: remain MCP/standalone
        |
        +--> if strong:
                 stabilize generic harness SDK
                 maintain useful public fork
                 propose upstream-friendly provider seam
                 support additional harnesses
```

The guiding rule is:

> **Do not ask OpenAI—or any harness maintainer—to believe the architecture. Build the public fork, keep the integration clean, and let reproducible benchmark results establish whether native SyntaxMesh intelligence deserves to become part of the harness.**

---

# 228. v4 research references

Current integration design was informed by the public OpenAI Codex repository as observed on 2026-09-23, including:

```text
openai/codex

codex-rs/Cargo.toml
    Rust Cargo workspace structure

codex-rs/core/README.md
    codex-core as shared business logic

codex-rs/tools/README.md
    host-side tool machinery and extension boundaries

codex-rs/core/src/context_manager/history.rs
codex-rs/core/src/context/world_state/*
    host-owned context/world-state lifecycle

codex-rs/core/src/tools/*
    tool planning, execution, approval and patch integration

codex-rs/apply-patch/*
    verified patch parsing/application support

codex-rs/ext/agent/*
codex-rs/codex-home/src/instructions/*
    extension/provider patterns
```

These are research observations, not promises of stable upstream APIs.

Before implementing the fork, re-check current upstream main and design against the actual contemporary extension/provider seams.

---

# 229. v4 final decision summary

```text
SyntaxMesh remains runtime agnostic:       YES
SyntaxMesh depends on Codex:                NO
Codex reference adapter depends on both:    YES
Public Codex fork appropriate:              YES, as experiment
MCP baseline required:                      YES
Native sidecar experiment:                  YES
Direct Rust embedding experiment:           YES, after baseline
Worktree-aware shared graph:                REQUIRED for serious multi-agent test
Automatic task context:                     EXPERIMENTAL, benchmarked
Automatic semantic verification:            HIGH-VALUE experiment
Change Engine included initially:            NO
OpenAI upstream integration assumed:         NO
Upstreamability optimized for:               YES
Generic harness SDK long-term:               YES

Primary proof target:
    correctness + reduced rediscovery + context efficiency

Primary anti-goal:
    a permanent invasive Codex fork whose value cannot be separated
    from benchmark-specific prompting or hard-coded repository knowledge
```

The v4 addition does not change the underlying SyntaxMesh product thesis.

It gives that thesis a concrete, falsifiable adoption experiment:

```text
Can a coding harness become materially better when a persistent,
incremental, evidence-backed engineering world model is native to the loop?
```

The Codex fork should answer that with measurements.

---

# 230. v5 thesis — temporal system intelligence, not merely snapshots

v4 already makes Git, graph generations, semantic generations, historical analytics, and semantic diff first-class.

v5 strengthens the history model substantially.

The core requirement is no longer only:

```text
"What did the graph look like at commit X?"
```

It is also:

```text
"What changed?"
"What caused that conclusion to change?"
"What depended on the change?"
"What changed later because a premise introduced here became part of the system?"
"When did a behavior become possible?"
"When was it first observed?"
"When was it last observed?"
"Which later engineering changes depended on this earlier change?"
"How did the blast radius evolve for months after the original change?"
"What does current SyntaxMesh semantics conclude about an old system state?"
"What did SyntaxMesh itself conclude at that historical point in time?"
```

The important distinction is:

```text
VERSION HISTORY
= a sequence of states

CHANGE LINEAGE
= explicit transitions between states

CONSEQUENCE LINEAGE
= evidence-backed relationships showing how one transition
  altered later facts, requirements, behaviors, contracts, or changes

TEMPORAL ANALYTICS
= queries over that history and lineage
```

SyntaxMesh MUST support all four without claiming unsupported causality.

The long-term temporal product thesis is:

> **SyntaxMesh should be able to reconstruct how an engineering system became what it is, which earlier changes contributed to later states, when important behavior became possible or observable, and how the consequences of a change propagated across code, documentation, schemas, architecture, runtime evidence, and subsequent engineering work.**

This is not a "butterfly effect feature."

If a small early change eventually produces a surprising wide downstream effect, that pattern should be **discoverable from the normal temporal model**.

---

# 231. Historical state is broader than Git

Git remains an important source of history, but Git commits are not the complete history of an engineering system.

SyntaxMesh history may include versioned changes to:

```text
source files
generated source
schemas
database migrations
OpenAPI/protobuf/GraphQL contracts
configuration
Terraform / infrastructure
CI definitions
build metadata
ADRs
RFCs
design documents
catalog/ownership metadata
architecture declarations
ontology versions
inference-rule versions
contract/policy versions
runtime observations
test observations
coverage observations
incidents
root-cause findings
deployment topology evidence
external provider evidence
```

Therefore:

```text
Git commit != SyntaxMesh history generation
```

A Git commit can cause one or more graph/semantic generations.

A runtime-observation batch can advance historical evidence without any Git commit.

An ontology upgrade can change derived semantic knowledge while source code remains byte-identical.

A documentation or architecture declaration can alter intended-system state without changing implementation state.

A production incident can add evidence about historical behavior without altering source code.

Keep these timelines related but distinct.

---

# 232. Temporal identity hierarchy

Use several explicit identities rather than forcing all history into one integer.

Conceptually:

```rust
RepositoryHistoryId
GraphGenerationId
SemanticGenerationId
EvidenceGenerationId
ChangeSetId
ChangeEventId
ObservationBatchId
ScenarioId
```

A useful relationship is:

```text
RepositoryHistory
    |
    +-- GraphGeneration G100
    |      |
    |      +-- SemanticGeneration S100.1
    |      +-- EvidenceGeneration E100.1
    |
    +-- GraphGeneration G101
           |
           +-- SemanticGeneration S101.1
           +-- SemanticGeneration S101.2
           +-- EvidenceGeneration E101.1
           +-- EvidenceGeneration E101.2
```

Do not assume:

```text
one Git commit
=
one graph generation
=
one semantic generation
=
one evidence generation
```

That will not remain true in a live, runtime-enriched system.

---

# 233. First-class ChangeSet identity

A Git commit is a version-control event.

A logical engineering change may span many events and repositories.

Introduce a first-class logical `ChangeSet`.

Conceptually:

```rust
pub struct ChangeSet {
    pub id: ChangeSetId,
    pub kind: ChangeSetKind,
    pub title: Option<String>,
    pub originating_intent: Option<EntityRef>,
    pub parent_changes: Vec<ChangeSetId>,
    pub git_commits: Vec<CommitRef>,
    pub pull_requests: Vec<ExternalRef>,
    pub issues: Vec<ExternalRef>,
    pub adrs: Vec<EntityRef>,
    pub repositories: Vec<RepositoryId>,
    pub first_generation: GenerationRef,
    pub last_generation: Option<GenerationRef>,
    pub provenance: ProvenanceSetId,
}
```

Potential kinds:

```text
FEATURE
BUG_FIX
REFACTOR
MIGRATION
SECURITY_FIX
ARCHITECTURE_CHANGE
DOCUMENTATION_CHANGE
POLICY_CHANGE
SCHEMA_CHANGE
DEPENDENCY_UPGRADE
ROLLBACK
GENERATED_CHANGE
AGENT_CHANGE
MANUAL_GROUP
UNKNOWN
```

A `ChangeSet` is not automatically inferred from adjacent commits.

Construction sources may include:

```text
explicit user declaration
PR identity
issue/PR linkage
commit trailers
branch metadata
Change Engine execution ID
Codex/agent task ID
migration manifest
release metadata
heuristic grouping
```

Every grouping method retains provenance.

A heuristic grouping must remain distinguishable from an explicitly declared logical change.

---

# 234. ChangeEvent — the atomic historical transition

A `ChangeSet` groups logical work.

A `ChangeEvent` records a concrete state transition.

Conceptually:

```rust
pub struct ChangeEvent {
    pub id: ChangeEventId,
    pub change_set: Option<ChangeSetId>,
    pub generation_before: GenerationRef,
    pub generation_after: GenerationRef,
    pub event_kind: ChangeEventKind,
    pub changed_entities: Vec<EntityRef>,
    pub changed_facts: FactDeltaRef,
    pub changed_claims: ClaimDeltaRef,
    pub changed_contract_results: ContractDeltaRef,
    pub changed_flows: FlowDeltaRef,
    pub provenance: ProvenanceSetId,
}
```

Examples:

```text
SOURCE_EDIT
SCHEMA_EDIT
DOC_EDIT
DECLARATION_EDIT
ONTOLOGY_UPDATE
RULE_UPDATE
RUNTIME_OBSERVATION
INCIDENT_EVIDENCE
GENERATED_ARTIFACT_UPDATE
REINDEX_REINTERPRETATION
```

This creates a stable substrate for historical reasoning without requiring SyntaxMesh to pretend that every transition came from source control.

---

# 235. Bitemporal-style fact semantics

Historical engineering intelligence needs at least two notions of time.

## 235.1 Valid/system time

When was this fact true in the modeled engineering system?

Example:

```text
dependency ServiceA -> ServiceB
valid from G120
valid until G184
```

## 235.2 Observation/knowledge time

When did SyntaxMesh learn or accept the evidence?

Example:

```text
runtime behavior occurred on September 10
telemetry imported on September 12
```

These are different.

Conceptually:

```rust
pub struct TemporalValidity {
    pub valid_from: GenerationRef,
    pub valid_until: Option<GenerationRef>,
    pub observed_at: ObservationTime,
    pub accepted_at: SemanticGenerationRef,
}
```

Do not necessarily implement a full generic bitemporal database abstraction.

But the logical model MUST allow SyntaxMesh to distinguish:

```text
when a fact was true
```

from:

```text
when SyntaxMesh learned that it was true
```

This is required for correct incident analysis, delayed telemetry, imported historical repositories, and retrospective semantic reconstruction.

---

# 236. Version every important fact class

Stable identity and temporal versions should apply beyond symbols.

Version:

```text
nodes
edges
concept mappings
claims
contracts
contract results
architecture declarations
behavioral flows
data-lineage relationships
ownership facts
deployment relationships
runtime observations
incident relationships
documentation claims
ontology concepts
ontology relations
inference rules
policies
verification predicates
```

Prefer:

```text
StableIdentity
    |
    +-- Version A
    +-- Version B
    +-- Version C
```

over allocating unrelated logical identities every time the representation changes.

This is essential for continuity questions such as:

```text
"How did this concept evolve?"
```

rather than merely:

```text
"Which records existed?"
```

---

# 237. State timeline queries

Expose ordinary version-history operations as typed queries.

Potential operations:

```text
History(entity)
History(concept)
History(claim)
History(contract)
History(flow)
History(change_set)
StateAt(entity, generation)
GraphAt(generation)
SemanticStateAt(generation)
EvidenceAt(generation)
ChangedBetween(a, b)
ChangedSince(generation)
```

Examples:

```text
Show the history of CustomerIdentity.

How did the login flow change from v1.4 to v2.0?

What contracts applied to PaymentService at release 8?

What did SyntaxMesh know about this dependency before incident INC-82?

Which representations of Money existed at generation G500?
```

---

# 238. Historical interpretation modes

A subtle but critical distinction must remain explicit.

There are at least two valid historical questions.

## 238.1 Historical conclusion mode

```text
"What did SyntaxMesh conclude at the time?"
```

Evaluate using the historical:

```text
graph generation
ontology version
rule set
declarations
contracts
semantic configuration
available evidence
```

This reproduces the historical conclusion.

## 238.2 Current-semantics retrospective mode

```text
"What does current SyntaxMesh knowledge conclude about that old system state?"
```

Use:

```text
old canonical source/system state
+
current ontology/rules/analysis capabilities
```

This can discover relationships that old SyntaxMesh versions did not know how to model.

Every historical query MUST label which mode produced the result.

Never silently reinterpret old states under new semantics and present that as what was known then.

---

# 239. Consequence lineage — core v5 addition

Introduce explicit relationships between change events and later consequences.

A consequence is not simply any later change.

Conceptually:

```rust
pub struct ConsequenceEdge {
    pub source: ChangeOrFactRef,
    pub target: ChangeOrFactRef,
    pub kind: ConsequenceKind,
    pub evidence: EvidenceSetId,
    pub derivation: DerivationKind,
    pub temporal_distance: GenerationDistance,
}
```

Initial consequence classes:

```text
DIRECT_CHANGE
DIRECT_DEPENDENCY_EFFECT
GENERATED_ARTIFACT_EFFECT
DERIVED_SEMANTIC_EFFECT
CONTRACT_EFFECT
BUILD_EFFECT
TEST_EFFECT
DEPLOYMENT_EFFECT
RUNTIME_OBSERVED_EFFECT
DECLARED_MIGRATION_EFFECT
CHANGE_DEPENDENCY
HISTORICALLY_CORRELATED
POSSIBLE_DOWNSTREAM_EFFECT
```

These are intentionally not all "causal."

The type tells the caller what SyntaxMesh actually knows.

---

# 240. Causality discipline

SyntaxMesh MUST be conservative about causal claims.

Never derive:

```text
A caused B
```

merely because:

```text
A happened before B
```

Strong consequence edges may be supported by:

```text
explicit generated-from metadata
dependency invalidation
compiler/build dependency
schema/generated-client relationship
declared migration step
Change Engine plan dependency
direct fact derivation
explicit issue/PR relationship
runtime trace lineage
StateChronicle/Penelope workflow linkage
```

Weaker relationships may be:

```text
temporal correlation
co-change history
shared concept involvement
statistical association
```

Those remain labeled as such.

The provenance should allow a caller to answer:

```text
"Why does SyntaxMesh believe this later change depends on the earlier one?"
```

---

# 241. Change-to-change dependency graph

Later engineering changes frequently depend on facts introduced by earlier changes.

Model this explicitly.

Example:

```text
Change C42
introduces API v2

Change C51
updates mobile client to API v2

Change C63
removes API v1

Change C71
removes compatibility adapter
```

The history graph may contain:

```text
C51 --DEPENDS_ON_CHANGE--> C42
C63 --DEPENDS_ON_CHANGE--> C51
C71 --DEPENDS_ON_CHANGE--> C63
```

Evidence may come from:

```text
symbol/schema dependency
explicit migration manifest
Change Engine plan
PR dependency
generated artifact lineage
semantic contract transition
```

This allows a historical query to discover that a small earlier change acquired large long-term consequences without defining a special "butterfly effect" object.

---

# 242. Multi-generation consequence propagation

Ordinary impact analysis asks:

```text
"What would this affect now?"
```

v5 also needs:

```text
"What did this actually affect over subsequent generations?"
```

Define a temporal propagation query.

Conceptually:

```text
ConsequenceTrace {
    origin,
    from_generation,
    until_generation,
    max_hops,
    included_kinds,
    evidence_threshold,
}
```

The result is a provenance-backed DAG, not a flat list.

Example:

```text
Change C42
   |
   +-- DIRECT_DEPENDENCY_EFFECT --> OpenAPI schema
   |                                  |
   |                                  +-- GENERATED_ARTIFACT_EFFECT --> TS SDK
   |                                                                         |
   |                                                                         +-- CHANGE_DEPENDENCY --> frontend migration C51
   |
   +-- CONTRACT_EFFECT --> compatibility contract introduced
                                      |
                                      +-- CHANGE_DEPENDENCY --> adapter removal C71
```

The surprising extent of a propagation emerges from this trace.

---

# 243. Delayed consequences

Some effects do not appear immediately.

Examples:

```text
a schema change becomes relevant only after a new consumer appears
a deprecated API remains harmless until traffic shifts
a capability expansion enables a later unsafe path
a config option changes semantics months before a feature starts using it
a new dependency creates a cycle only after another package is added
```

SyntaxMesh should support delayed consequence discovery.

Represent:

```text
origin change
    |
latent enabling fact
    |
later triggering change
    |
observed consequence
```

Do not force the entire consequence onto the origin.

Example:

```text
C10 introduces broad interface
C81 adds implementation
C95 starts calling new implementation
INC-12 reveals unexpected effect
```

The correct historical explanation may be:

```text
C10 enabled the possibility
C81 instantiated it
C95 activated it
INC-12 observed the failure
```

That is much more useful than claiming:

```text
"C10 caused INC-12"
```

---

# 244. Enabling, activating, and observing are separate historical relations

Introduce explicit semantics such as:

```text
ENABLES
ACTIVATES
OBSERVES
RESOLVES
SUPERSEDES
REINTRODUCES
REGRESSES
```

Example:

```text
commit A
    ENABLES path P

commit B
    ACTIVATES path P

runtime trace T
    OBSERVES path P

commit C
    RESOLVES path P
```

This gives SyntaxMesh a precise vocabulary for long-range engineering history.

---

# 245. First-possible / first-observed / last-observed queries

Support first-class temporal questions:

```text
FirstPossible(behavior_or_path)
FirstObserved(behavior_or_path)
LastObserved(behavior_or_path)
FirstDeclared(concept_or_contract)
FirstImplemented(concept_or_contract)
FirstViolation(contract)
LastViolation(contract)
ResolvedAt(violation_or_claim)
```

Example:

```text
FirstPossible:
G5812

Reason:
OrderService gained direct InventoryRepository access.

FirstObserved:
G5839 / runtime batch R221

LastObserved:
G5910

Resolved:
G5918 after C991
```

This separates static possibility from actual runtime behavior.

That is essential for incident/root-cause analysis.

---

# 246. Historical origin queries — semantic blame

Git blame answers:

```text
"Who last changed this line?"
```

SyntaxMesh should additionally answer:

```text
"Why does this architecture/fact/contract exist?"
```

Potential operations:

```text
Origin(fact)
Origin(claim)
Origin(contract)
Origin(concept_mapping)
Origin(flow)
DecisionLineage(entity)
```

Example:

```text
Current:
LedgerWrite requires AuthorizationCapability

Lineage:
ADR-52
    -> ChangeSet C184
    -> PaymentGateway refactor
    -> TrustGrant integration
    -> Contract finance:authorized-ledger-write
    -> current derived requirement
```

This is semantic provenance across time.

---

# 247. Documentation-to-implementation consequence tracking

Documents are not passive attachments.

ADRs, RFCs, architecture declarations, and migration plans may intentionally drive later implementation.

Allow relationships:

```text
ADR --MOTIVATES--> ChangeSet
RFC --PROPOSES--> ChangeSet
MigrationPlan --ORDERS--> ChangeSet
ChangeSet --IMPLEMENTS--> ADR
ChangeSet --PARTIALLY_IMPLEMENTS--> ADR
ChangeSet --CONTRADICTS--> ADR
ChangeSet --SUPERSEDES_DECISION--> ADR
```

Then queries can ask:

```text
What changed after ADR-52?

Which parts of ADR-52 were eventually implemented?

Which later commits contradicted that decision?

How long did it take for the implementation to converge on the declared architecture?
```

Never assume temporal succession means document causation.

Require explicit or derived evidence.

---

# 248. Implementation-to-documentation consequence tracking

The inverse is also useful.

Source changes may cause:

```text
docs update
API documentation regeneration
ADR revision
runbook change
incident postmortem
architecture model update
```

Track this direction separately.

Example:

```text
ChangeSet C200
    -> source API change
    -> generated OpenAPI diff
    -> docs regenerated
    -> migration guide added
```

This helps answer:

```text
Which documentation became stale after this code change?
```

and historically:

```text
How long was documentation stale before it was corrected?
```

---

# 249. Cross-layer consequence lineage

A long-range consequence often crosses multiple evidence layers.

Example:

```text
source change
   -> schema change
      -> generated SDK change
         -> client behavior change
            -> runtime traffic shift
               -> latency increase
                  -> incident
                     -> architecture decision
                        -> later refactor
```

SyntaxMesh should be able to preserve that complete lineage while retaining provenance for each edge.

Potential layers:

```text
SOURCE
BUILD
SCHEMA
GENERATED
API
DOCUMENTATION
ARCHITECTURE
DEPLOYMENT
RUNTIME
INCIDENT
POLICY
FOLLOW_UP_CHANGE
```

This is one of the main reasons the v5 history model must extend beyond Git.

---

# 250. Cross-repository historical propagation

The consequence graph MUST work across repositories.

Example:

```text
backend repo
    API schema change
        |
        v
SDK repo
    generated client
        |
        v
mobile repo
    migration
        |
        v
infra repo
    rollout config
```

A single logical ChangeSet may span all four.

Stable concept identity and contract identity should allow the lineage to survive repository boundaries.

Do not require all repositories to share a Git history.

---

# 251. Blast-radius evolution through time

Current blast radius is only one point on a trajectory.

Provide historical metrics such as:

```text
BlastRadiusHistory(entity_or_concept)
```

Possible dimensions:

```text
direct callers
transitive callers
services
repositories
APIs
schemas
tables
events
runtime participants
flows
contracts
deployments
tests
owners
```

Example:

```text
AuthService

release   callers  services  flows  contracts  repos
1.0          5        2        2       1        1
1.5         11        5        7       4        3
2.3         28       12       16       9        8
```

The result may show architectural centralization or coupling accumulation.

Do not collapse the dimensions into one opaque "risk score" unless a user-defined scoring policy requests it.

---

# 252. Consequence-growth curves

For a specific logical change, expose how its observed consequence set grew over time.

Conceptually:

```text
ConsequencesOverTime(change_set)
```

Possible output:

```text
G100  direct entities:       7
G105  dependent entities:   19
G120  dependent entities:   33
G170  affected repos:        5
G220  remaining legacy debt: 2
G260  legacy debt:           0
```

This is useful for:

```text
migrations
platform changes
API transitions
security hardening
large refactors
deprecations
```

A large delayed expansion may look like a "butterfly effect," but SyntaxMesh simply reports the evidence-backed propagation.

---

# 253. Long-range consequence discovery

SyntaxMesh should support finding unexpectedly distant propagation without requiring the caller to know the endpoint.

Potential query:

```text
LongRangeConsequences(
    origin,
    min_generation_distance,
    max_generation_distance,
    evidence_classes,
    semantic_scope
)
```

Rank by:

```text
lineage evidence strength
semantic distance
generation distance
cross-layer transitions
cross-repository transitions
runtime confirmation
number of intermediate dependencies
rarity/unexpectedness of the path
```

The ranking is for exploration.

It must never transform a weak correlation into a strong causal claim.

---

# 254. Surprise / anomaly in historical propagation

A useful analytical layer may flag consequences that were:

```text
far away in graph distance
far away in time
cross-domain
cross-repository
not predicted by original impact analysis
later runtime-confirmed
```

Possible finding:

```text
"Change C42 eventually affected the billing export pipeline
 47 generations later through three independently introduced dependencies."
```

The finding MUST show the exact path and evidence.

Do not persist a special `BUTTERFLY_EFFECT` fact.

The value lies in discoverable lineage, not terminology.

---

# 255. Predicted impact vs realized impact

SyntaxMesh already computes prospective impact.

v5 should retain the predicted impact set so it can later compare against realized history.

Model:

```text
At G100:
PredictedImpact(Change C42) = {A, B, C}

By G180:
RealizedImpact(Change C42) = {A, B, C, D, E}
```

Then analyze:

```text
predicted and realized
predicted but never realized
realized but not predicted
```

This is valuable for improving:

```text
impact algorithms
ontology
dependency models
agent context selection
change planning
human review
```

Do not punish a prediction system for correctly expressing uncertainty.

Store confidence/evidence classes with the original prediction.

---

# 256. Historical false-negative discovery

A particularly high-value query:

```text
Which later consequences were not visible to SyntaxMesh at the time?
```

Potential causes:

```text
missing repository
missing runtime evidence
missing ontology mapping
dynamic dependency
new downstream consumer created later
incomplete analyzer
latent capability not yet used
```

This can drive SyntaxMesh hardening.

Example:

```text
C42 later affected Service Z,
but Service Z did not exist at C42.

Classification:
NOT PREDICTABLE AT ORIGIN
DEPENDENCY INTRODUCED LATER
```

Versus:

```text
Service Z existed and depended on affected schema,
but SyntaxMesh missed the relation.

Classification:
MODEL/ANALYZER FALSE NEGATIVE
```

Those are fundamentally different.

---

# 257. Historical false-positive discovery

Similarly:

```text
Which predicted impacts never materialized?
```

Possible reasons:

```text
dead code
unused API
feature flag
alternative implementation path
migration avoided
runtime path never active
impact over-approximation
```

Feed this evidence into benchmark/evaluation systems.

Never automatically remove static possibility just because runtime never observed it.

---

# 258. Change dependency vs entity dependency

Keep these graph types separate.

Entity dependency:

```text
ServiceA DEPENDS_ON LibraryB
```

Change dependency:

```text
MigrationC51 DEPENDS_ON ChangeC42
```

A later change can depend on an earlier change even when the final entities no longer have a direct dependency.

Example:

```text
C42 introduces temporary adapter
C51 migrates consumers through adapter
C63 removes old protocol
C71 removes adapter
```

At the final state, the adapter no longer exists.

The engineering-history dependency remains important.

---

# 259. Migration lineage as a first-class use case

Large migrations are ideal consumers of v5 temporal intelligence.

Track:

```text
migration intent
migration phases
compatibility bridges
producer changes
consumer changes
data migration
traffic migration
deprecation
legacy removal
verification evidence
```

Queries:

```text
Which migration phase introduced this artifact?

Which consumers are still on the old representation?

When was the old representation last observed?

Which compatibility layer can now be removed?

What later changes depended on the temporary migration bridge?
```

This is particularly useful for coding agents that encounter apparently redundant compatibility code.

---

# 260. Regression lineage

Support:

```text
IntroducedBy(violation)
ResolvedBy(violation)
ReintroducedBy(violation)
```

Example:

```text
Architecture contract C7

introduced violation: Change C200
resolved:             Change C214
reintroduced:         Change C330
resolved:             Change C335
```

Historical analysis can then answer:

```text
Which contracts repeatedly regress?
Which subsystems repeatedly reintroduce the same dependency?
Which fixes are unstable?
```

---

# 261. Runtime aftermath windows

For deployment-associated changes, support before/after evidence windows.

SyntaxMesh is not an APM and should not ingest unlimited telemetry.

But an external runtime provider may summarize:

```text
before change
after change
stabilized period
```

Example evidence:

```text
old API usage:
before: 18k/day
after migration: 3k/day
after cleanup: 0/day
```

This can attach runtime aftermath to a ChangeSet without turning SyntaxMesh into a time-series warehouse.

---

# 262. Incident temporal reconstruction

Incident analysis should be able to reconstruct:

```text
system state before incident
changes since last healthy state
first generation where failure path was possible
first runtime observation
incident declaration
mitigation change
permanent fix
post-fix runtime evidence
```

Potential query:

```text
IncidentTimeline(INC-991)
```

Example:

```text
G500 healthy
G512 path becomes statically possible
G531 new deployment activates path
R881 first observed failure
INC-991 opened
C700 mitigates
C711 permanently removes path
R910 confirms path absent
```

This provides much stronger root-cause context than a flat list of recent commits.

---

# 263. Versioned documentation and rationale

Documentation should retain semantic identity across edits where possible.

Example:

```text
ADR-17 v1
ADR-17 v2
ADR-17 SUPERSEDED_BY ADR-42
```

Queries:

```text
What did the architecture document say when this code was written?

Which implementation still conforms to an obsolete ADR?

Which current docs refer to concepts that no longer exist?

Which architecture rule changed meaning because the ADR was revised?
```

Historical context packs for agents should prefer the document version that was valid for the code state being investigated.

---

# 264. Versioned ontology and rule lineage

Ontology evolution itself must be queryable.

Example:

```text
finance:Ledger
v1 -> broad stateful resource
v2 -> append-only financial resource
v3 -> split into SettlementLedger / AccountingLedger
```

Derived knowledge may change because the ontology changed rather than because code changed.

History must therefore be able to say:

```text
"the underlying source did not change;
the semantic classification changed under ontology v3."
```

Similarly for:

```text
inference rules
contracts
policy definitions
concept mappings
```

This is essential for reproducibility.

---

# 265. Historical stable-identity continuity

Renames/moves/refactors should preserve continuity when evidence is strong.

Example:

```text
PaymentService::refund
    ->
RefundCoordinator::execute
```

A naive history system sees deletion + creation.

SyntaxMesh should be able to represent:

```text
MOVED_AND_RENAMED
semantic continuity: high confidence
```

Then historical queries can follow the logical operation through refactors.

Sources:

```text
Git rename similarity
AST/body similarity
signature compatibility
call-site migration
concept mapping
explicit refactoring metadata
Change Engine lineage
```

Never silently merge ambiguous identities.

---

# 266. History over deleted entities

Deleting an entity must not make its historical identity inaccessible.

Queries such as:

```text
History(deleted_symbol)
Origin(deleted_contract)
WhyWasRemoved(entity)
WhatReplaced(entity)
```

must continue to work.

The live graph can omit dead entities from hot projections while historical storage retains their versions.

---

# 267. Historical path queries

Allow a path query at any generation:

```text
PathAt(G500, A, B)
PathAt(G900, A, B)
```

and a path-evolution query:

```text
PathHistory(A, B)
```

Example:

```text
G500:
A -> C -> B

G620:
A -> D -> B

G800:
no path

G920:
A -> E -> F -> B
```

This is useful for:

```text
architecture evolution
security-boundary analysis
migration verification
incident investigation
```

---

# 268. Temporal data-lineage queries

Data lineage changes over time.

Support:

```text
DataLineageAt(generation, source, sink)
DataLineageHistory(source, sink)
FirstDataFlow(source, sink)
LastDataFlow(source, sink)
```

Example:

```text
"When did customer email first become reachable by analytics?"
```

Result should show:

```text
static possibility generation
first runtime observation
relevant ChangeSets
contract state at the time
```

---

# 269. Temporal contract analysis

Contracts themselves and their evaluation results are historical facts.

Support:

```text
ContractHistory(contract)
ViolationHistory(contract)
ComplianceAt(generation, contract)
```

Example:

```text
C7 authorized-ledger-write

G100 PASS
G182 FAIL
G191 PASS
G330 FAIL
G335 PASS
```

Attach each transition to the relevant consequence/change lineage where evidence exists.

---

# 270. Temporal architecture drift

Drift should have a life cycle.

```text
drift introduced
drift first observed
drift acknowledged
drift accepted / declaration updated
drift resolved
drift regressed
```

Queries:

```text
How long did this subsystem remain out of conformance?

Which drift findings persisted longest?

Which declared architecture areas repeatedly diverge from implementation?
```

This makes architecture governance historical rather than snapshot-based.

---

# 271. Historical ownership and organizational context

Ownership changes over time too.

Track:

```text
declared owner
Git/change owner
review owner
operational owner
incident responder
```

Queries:

```text
Who owned this service when the decision was made?

Which team inherited this architectural debt?

Did ownership transfer precede or follow the subsystem refactor?
```

Do not infer personal responsibility or blame.

This is engineering context, not employee scoring.

---

# 272. Temporal provider contract

External providers should be able to submit historical evidence.

Potential API:

```rust
trait HistoricalEvidenceProvider {
    fn provider_identity(&self) -> ProviderIdentity;

    fn enumerate_range(
        &self,
        workspace: &WorkspaceRef,
        range: HistoryRange,
        sink: &mut dyn HistoricalEvidenceSink,
    ) -> Result<(), ProviderError>;
}
```

Every record should include:

```text
source timestamp
observation timestamp
provider version
trust class
stable subject identity
source reference
```

This enables importing historical:

```text
CI
coverage
APM
incident
catalog
architecture
security-analysis
```

evidence without making the provider authoritative over source facts.

---

# 273. Temporal extension semantics

Extensions may define their own historical change kinds and consequence relationships.

Example:

```text
statechronicle:RESOURCE_AUTHORITY_CHANGED
trustgrant:CAPABILITY_SCOPE_CHANGED
bevy:SCHEDULE_ORDER_CHANGED
acme:PAYMENT_RAIL_MIGRATED
```

Requirements:

- namespaced
- versioned
- schema validated
- provenance required
- compatible with generic history traversal
- no arbitrary code execution in query evaluation

Core temporal operations must still work for unknown extension record types where generic relationships exist.

---

# 274. Temporal query API additions

Add candidate typed query operations:

```text
History(entity_or_concept)
StateAt(generation, selector)
SemanticStateAt(generation, selector)

Origin(selector)
DecisionLineage(selector)

ChangeSet(id)
ChangeHistory(selector)
ChangeDependencies(change)
DependentChanges(change)

Consequences(change)
ConsequencesBetween(change, range)
LongRangeConsequences(change, range)

FirstPossible(selector)
FirstObserved(selector)
LastObserved(selector)

IntroducedBy(selector)
ResolvedBy(selector)
ReintroducedBy(selector)

BlastRadiusHistory(selector)
ConsequencesOverTime(change)

PathAt(generation, from, to)
PathHistory(from, to)

DataLineageAt(generation, source, sink)
DataLineageHistory(source, sink)

ContractHistory(contract)
ViolationHistory(contract)

DriftHistory(scope)

PredictedVsRealized(change)

HistoricalConclusion(generation, query)
RetrospectiveConclusion(generation, current_semantics, query)
```

Do not expose one giant temporal DSL initially.

Start with typed operations and add a textual DSL only when usage patterns justify it.

---

# 275. `explain` for historical conclusions

Historical queries need the same provenance standard as live queries.

Example:

```text
syntaxmesh explain <historical-claim-id>
```

should be able to show:

```text
historical generation
source/evidence versions
ontology version
rule version
ChangeSet lineage
supporting facts
contradicting facts
runtime observations
later supersession/resolution
```

An LLM may summarize this DAG.

The DAG remains canonical.

---

# 276. Timeline-friendly public DTOs

Public API DTOs should expose history without requiring consumers to understand internal storage.

Potential records:

```text
HistoryPoint
EntityVersion
FactVersion
ClaimVersion
ChangeSetRecord
ChangeEventRecord
ConsequenceRecord
ObservationRecord
TemporalContractResult
HistoricalExplanation
```

NDJSON exports may support:

```text
syntaxmesh export ndjson --history <selector>
syntaxmesh export ndjson --changes <range>
```

The format should stream.

Do not create a monolithic lifetime-history JSON object.

---

# 277. Historical storage architecture

Keep hot current state separate from heavy history where appropriate.

Conceptual split:

```text
Turso / SQLite-compatible live store
    current canonical facts
    recent version indexes
    generation metadata
    lineage indexes needed for hot queries

DuckDB / Parquet analytical history
    long time ranges
    trend analysis
    consequence-growth analytics
    aggregate historical comparisons

optional immutable artifacts
    Shardline snapshots / exports / checkpoints
```

Do not make every historical query require replaying the repository from Git.

Store sufficient canonical deltas/checkpoints for practical reconstruction.

---

# 278. Checkpoint + delta strategy

Avoid storing full duplicate semantic worlds for every generation.

Use:

```text
periodic checkpoint
+
deterministic deltas
+
structural sharing
```

Potential reconstruction:

```text
Checkpoint G1000
    +
delta G1001
    +
delta G1002
    +
...
    ->
G1012
```

Benchmark checkpoint cadence.

Criteria include:

```text
reconstruction latency
storage size
delta density
historical query frequency
repository scale
```

---

# 279. History retention and compaction

Not every raw runtime observation must live forever in the canonical history store.

Retention tiers may be:

```text
RAW
SUMMARIZED
CHECKPOINTED
ARCHIVED
```

Example:

```text
raw traces         -> external APM
SyntaxMesh stores  -> summarized dependency evidence
```

For semantic lineage, retain enough to reproduce:

```text
claim state
consequence relationship
provider identity
source reference
aggregate evidence
```

Do not silently destroy evidence required for published/verified historical claims.

---

# 280. StateChronicle role in historical verification

StateChronicle can optionally verify published SyntaxMesh generation lineage.

Potential publication unit:

```text
HistoricalGenerationManifest {
    graph_generation_root,
    semantic_generation_root,
    change_delta_root,
    consequence_delta_root,
    provider_set_hash,
    configuration_hash,
}
```

Do not record every temporal edge independently unless a concrete use case requires it.

The correct boundary remains accepted/published generations or historical artifacts.

---

# 281. Penelope role in historical processing

Historical ingestion and recomputation can be long-running workflows.

Penelope may coordinate:

```text
historical repository import
generation backfill
external evidence backfill
ontology re-evaluation of history
consequence-lineage rebuild
DuckDB synchronization
history compaction
checkpoint publication
```

Do not put Penelope in individual temporal read queries.

---

# 282. Historical re-index / backfill semantics

When a new analyzer becomes available, users may choose to backfill history.

Example:

```text
new Rust resolver v4
```

Modes:

```text
NO_BACKFILL
FUTURE_ONLY
SELECTED_RANGE
FULL_HISTORY
```

Backfilled conclusions MUST be marked as retrospective analysis.

Do not rewrite what SyntaxMesh historically knew at the time.

Preserve both:

```text
historical-original conclusion
retrospective-improved conclusion
```

---

# 283. History import from existing repositories

SyntaxMesh should be able to bootstrap useful history from an existing Git repository.

Do not require indexing every commit naïvely.

Possible strategy:

```text
select release/merge checkpoints
index high-value commits
reuse unchanged file extraction by content hash
reconstruct deltas
increase fidelity on demand
```

Support user-selected:

```text
all releases
last N months
main-branch merges
specific incident window
specific migration window
```

Use content-addressed extraction caches aggressively.

---

# 284. Adaptive historical fidelity

History does not need identical resolution everywhere.

Possible fidelity levels:

```text
GENERATION_METADATA_ONLY
STRUCTURAL
SEMANTIC
FULL_EVIDENCE
```

A repository can keep:

```text
recent 90 days = FULL_EVIDENCE
last 2 years   = SEMANTIC
older history  = STRUCTURAL / release checkpoints
```

Queries must report available fidelity.

Do not fabricate absent evidence.

---

# 285. Change lineage for working trees and agent sessions

History is not only committed Git.

For agentic coding:

```text
agent task
    |
worktree generation W1
    |
edit batch E1
    |
semantic verification
    |
edit batch E2
    |
tests
    |
commit
```

SyntaxMesh may record ephemeral change lineage for the active task.

If discarded, retention policy may delete it.

If accepted/committed, it can be linked to the durable ChangeSet.

This helps answer:

```text
Which agent edit introduced the failed contract?
Which repair removed it?
What context pack was used before the incorrect edit?
```

---

# 286. Codex-native history integration

The v4 Codex experiment should use temporal intelligence where useful.

Potential native harness operations:

```text
repo.history
repo.origin
repo.first_possible
repo.first_observed
repo.blast_history
repo.change_dependencies
repo.consequences
```

Before modifying apparently obsolete code, Codex can ask:

```text
Why does this exist?
When was it introduced?
What migration depended on it?
Is the reason still active?
When was it last observed?
```

This is substantially more useful than generic Git blame.

Do not automatically inject large historical context into every task.

History should be pulled when relevant or selected by the context compiler.

---

# 287. History-aware context compiler

The context compiler should rank historical evidence by relevance.

Possible high-value historical context:

```text
originating ADR
incident that introduced defensive logic
migration that created compatibility code
last observed use of a deprecated API
contract change affecting current task
previous regression of same invariant
recent change that altered current blast radius
```

Avoid:

```text
dumping full Git history
dumping every commit message
adding unrelated old incidents
```

The goal remains maximum useful engineering information per token.

---

# 288. Historical context budget

Add token-budget scoring for history:

```text
relevance
* causal/lineage evidence strength
* semantic importance
* recency where appropriate
* uniqueness
/ token cost
```

A 6-year-old ADR can outrank a recent commit if it defines the invariant relevant to the current change.

Do not use recency as the sole ranking signal.

---

# 289. Change Engine integration boundary

The separate SyntaxMesh Change Engine may consume v5 history.

Good uses:

```text
avoid repeating failed migration pattern
detect existing migration lineage
determine whether compatibility artifact is still needed
identify required transition order
reuse historical verification predicates
estimate likely downstream migration work
```

SyntaxMesh remains responsible for:

```text
historical facts
lineage
semantic requirements
consequence evidence
```

Change Engine remains responsible for:

```text
executable plan
transformation execution
repair/replan loop
```

Do not move temporal truth into the Change Engine.

---

# 290. Historical scenario comparison

Named future-state scenarios can also be compared against historical trajectories.

Questions:

```text
Has this architecture pattern been tried before?

Which previous migration most resembles this scenario?

What downstream effects followed the last time we moved authority this way?

Which old scenario eventually became the current architecture?
```

This is retrieval/analysis over history, not proof that history will repeat.

---

# 291. Branch trajectory comparison

Compare two evolving branches, not only two snapshots.

Example:

```text
Trajectory(feature-a)
vs
Trajectory(feature-b)
```

Potential output:

```text
both modify CustomerIdentity
feature-a increases service coupling
feature-b adds compatibility layer
feature-a violates C7 at G4
feature-b remains compatible through G8
```

Useful for architecture experiments and alternative implementations.

---

# 292. Release-to-release propagation analysis

For releases:

```text
release R1
    -> ChangeSets introduced
    -> contracts affected
    -> runtime behavior changed
    -> incidents
    -> follow-up fixes
```

A release report can answer:

```text
Which changes continued generating follow-up work after release?
Which migrations were still incomplete?
Which architectural effects appeared only in later releases?
```

---

# 293. Technical-debt lineage

Technical debt can be represented as historical claims with origins and dependents.

Example:

```text
Temporary compatibility adapter
introduced by C42
expected removal after C63

still present at G900

remaining blockers:
C58 consumer not migrated
```

This is far more useful than a static TODO comment.

A coding agent can understand whether the "temporary" artifact is actually removable.

---

# 294. Deprecation lifecycle

First-class deprecation history:

```text
introduced
deprecated
replacement available
new consumers forbidden
old consumers migrated
last runtime use
removed
```

Queries:

```text
Which deprecated APIs gained new consumers after deprecation?

Which deprecated entities are safe to remove?

Which deprecations have stalled longest?
```

---

# 295. Security-fix lineage

Security-sensitive historical analysis may track:

```text
vulnerability finding
affected path
fix ChangeSet
new contract/policy
regression tests
runtime confirmation
later regression
```

Do not expose secret vulnerability details beyond configured access controls.

Historical provenance is still subject to workspace security policy.

---

# 296. Performance requirements for temporal queries

Do not let v5 turn every query into a lifetime graph traversal.

Latency classes:

```text
HOT
current state
recent generation diff
origin lookup
last-change lookup

WARM
entity history
change dependencies
blast-radius history
recent consequence trace

COLD
multi-year consequence discovery
full historical retrospective analysis
large trajectory comparisons
history-wide semantic re-evaluation
```

Use:

```text
precomputed lineage indexes
DuckDB analytical projections
checkpoints
materialized summary tables
bounded traversal
```

where benchmarks justify them.

---

# 297. Temporal query bounds

Every potentially large historical traversal should support explicit bounds:

```text
generation range
calendar range
max hops
max changes
max entities
relation classes
repository scope
concept scope
evidence classes
minimum provenance/trust class
```

Return truncation information.

Never silently scan unlimited history in an interactive path.

---

# 298. Consequence-path explanation

A discovered long-range path should explain every hop.

Example:

```text
C42
  --introduced schema field-->
SchemaV2

SchemaV2
  --generated-->
SDKV2

SDKV2
  --adopted by-->
C51

C51
  --enabled runtime operation-->
Flow F

Flow F
  --first observed in-->
RuntimeBatch R88
```

Each edge carries:

```text
evidence
derivation
generation
producer
trust class
```

That is the difference between useful historical intelligence and storytelling.

---

# 299. No magical causality score

Do not hide historical reasoning behind one number like:

```text
causality = 0.83
```

If probabilistic ranking is useful, expose it only alongside:

```text
edge class
evidence classes
path
derivation
temporal distance
```

The explanation is more important than the score.

---

# 300. Temporal contradictions

Historical evidence may disagree.

Example:

```text
docs at G500:
ServiceA no longer calls ServiceB

source at G500:
call still present

runtime at G500:
call observed
```

Store the disagreement.

Later docs may catch up.

Queries should be able to answer:

```text
How long did documentation contradict implementation?
```

This is valuable system intelligence.

---

# 301. Delayed evidence arrival

Runtime, incident, or external-provider evidence can arrive after the modeled period.

Example:

```text
event occurred at T1
evidence imported at T3
```

Do not rewrite the observation timestamp.

Store:

```text
valid/event time = T1
knowledge/import time = T3
```

This is one reason v5 needs explicit observation time.

---

# 302. Historical provenance durability

A historical conclusion must remain explainable even if:

```text
current plugin version changed
current ontology changed
current repository deleted old branch
external provider rotated retention
```

For published historical claims, retain enough durable provenance metadata or immutable artifacts to reconstruct the explanation.

Do not require indefinite retention of all raw external telemetry.

---

# 303. Historical privacy and data minimization

History increases privacy risk.

Requirements:

- do not archive secret values merely because they appeared historically
- support redaction policies
- distinguish source identity from sensitive content
- avoid storing raw telemetry payloads where summaries suffice
- allow retention policies
- preserve hashes/references where content cannot be retained
- document what historical data leaves the machine when remote models/providers are enabled

Historical capability must not become accidental permanent sensitive-data retention.

---

# 304. Repository rewrite / force-push handling

Git history may be rewritten.

SyntaxMesh historical identity must not silently mutate.

When a force-push/rebase changes commit topology:

```text
old observed history
new repository history
```

Preserve the previous observed generation lineage if retained.

Mark commit refs:

```text
CURRENT
ORPHANED
REWRITTEN
REPLACED_BY
```

Do not silently pretend old verified analysis never existed.

---

# 305. Merge semantics

Merges complicate historical causality.

A merge may:

```text
combine independent changes
resolve conflicts
introduce new semantic state not present on either parent
```

Graph history should model parentage.

Consequence attribution should distinguish:

```text
inherited from parent A
inherited from parent B
introduced by merge resolution
```

Avoid attributing all merged effects to the merge commit itself.

---

# 306. Cherry-pick / backport identity

The same logical change may appear in multiple branches/releases.

Allow:

```text
ChangeSet C42

instances:
main commit abc
release/1.x commit def
release/2.x commit ghi
```

Then queries can compare whether downstream consequences differed by branch.

This is especially useful for security fixes and long-lived release lines.

---

# 307. Revert semantics

A revert is not equivalent to historical deletion.

Represent:

```text
C42 introduced fact F
C80 reverted C42
```

History retains both.

Possible later:

```text
C100 reintroduces F differently
```

Queries:

```text
When was this behavior active?
Why did it disappear?
Was it later reintroduced?
```

---

# 308. Generated artifacts and regeneration lineage

Generated artifacts often create confusing history.

Track:

```text
source schema
    GENERATES
SDK file

schema change
    CAUSES_REGENERATION
generated ChangeEvent
```

Then large generated diffs do not obscure the real origin.

Historical impact can collapse generated noise into its upstream semantic source where appropriate.

---

# 309. Build and dependency upgrade lineage

Dependency upgrades can create indirect consequences.

Track:

```text
dependency version change
API surface change
compile errors
changed transitive dependencies
security finding resolution
behavior/runtime shift
```

This allows:

```text
What later code changes were required because of this dependency upgrade?
```

Again, rely on evidence rather than chronology.

---

# 310. Test evolution lineage

Tests are part of engineering history.

Track:

```text
test introduced because of incident
test updated because behavior changed
test removed because feature removed
coverage shifted across concept
```

Useful queries:

```text
Which regression test protects this historical bug?

When was this invariant first covered?

Did the test disappear before the bug regressed?
```

---

# 311. Verification-predicate history

Verification predicates generated by SyntaxMesh are also versioned engineering knowledge.

Track:

```text
predicate introduced
predicate changed
predicate satisfied
predicate violated
predicate superseded
```

The Change Engine can reuse relevant historical predicates when similar migrations occur.

---

# 312. Agent-result lineage

For agentic development experiments, preserve optional links:

```text
task
context pack
model/provider/version
agent plan
tool actions
ChangeSet
verification result
repair iterations
```

This belongs to evaluation/provenance, not to canonical source truth.

It enables:

```text
Which historical context reduced agent rework?
Which missing consequence caused a bad first patch?
```

Follow privacy and retention policies.

---

# 313. Historical context quality benchmarks

Add benchmark tasks where success depends on history.

Examples:

```text
remove a legacy compatibility layer safely
fix a regression previously seen two years earlier
understand why a strange validation branch exists
continue a multi-release migration
identify when a security boundary was bypassed
explain a delayed downstream effect
```

Compare:

```text
agent + current source only
agent + Git history
agent + SyntaxMesh structural history
agent + SyntaxMesh semantic/consequence history
```

Measure:

```text
task success
incorrect removals
missed dependencies
tokens
tool calls
files opened
history queries
patch attempts
contract violations
human corrections
```

---

# 314. Consequence-discovery benchmark

Create known-ground-truth histories with:

```text
direct downstream effects
delayed effects
later-created consumers
cross-repo migrations
generated artifacts
false temporal correlations
reverts
branch divergence
runtime-only activation
```

Measure:

```text
direct-consequence precision/recall
change-dependency precision/recall
weak-correlation false-positive rate
first-possible accuracy
first-observed accuracy
origin-lineage accuracy
long-range consequence precision
```

The benchmark should penalize invented causality heavily.

---

# 315. Historical scalability benchmark

Measure:

```text
1k generations
10k generations
100k generations where practical

single repo
monorepo
multi-repo workspace

entity history latency
state reconstruction latency
origin lookup latency
consequence traversal latency
blast-history latency
DuckDB analytical latency
storage per generation
checkpoint rebuild cost
```

Do not optimize based only on synthetic microbenchmarks.

Include real repository histories.

---

# 316. v5 implementation stages

Do not attempt all temporal intelligence in the first release.

## Stage A — temporal primitives

Required architecture:

```text
stable generation IDs
entity/fact version validity
ChangeSet ID
ChangeEvent ID
historical provenance
original-vs-retrospective semantic mode
```

## Stage B — basic history

Implement:

```text
History
StateAt
ChangedBetween
Origin
IntroducedBy
ResolvedBy
FirstPossible
```

## Stage C — change lineage

Implement:

```text
change-to-change dependencies
direct consequence edges
generated-artifact lineage
migration lineage
historical contract results
```

## Stage D — runtime/historical evidence

Implement:

```text
FirstObserved
LastObserved
incident timeline
runtime aftermath summaries
delayed evidence handling
```

## Stage E — long-range analytical history

Implement:

```text
BlastRadiusHistory
ConsequencesOverTime
LongRangeConsequences
PredictedVsRealized
trajectory comparison
historical anomaly discovery
```

This keeps v5 from delaying the reliable structural foundation.

---

# 317. Schema reservations required early

Even if advanced history is deferred, reserve stable concepts for:

```text
ChangeSetId
ChangeEventId
GenerationRef
TemporalValidity
ConsequenceKind
ObservationTime
HistoricalInterpretationMode
```

Avoid designing the first database schema in a way that assumes:

```text
fact = timeless row
```

That assumption would become expensive to unwind later.

---

# 318. History remains rebuildable where possible

Deterministic historical state should be reproducible from:

```text
source revisions
extractor versions
resolver versions
ontology versions
rule versions
declarations
configuration
```

But exact historical runtime evidence may not be reproducible if the external source has expired.

Therefore distinguish:

```text
REBUILDABLE_HISTORY
RECORDED_HISTORY
EXTERNAL_REFERENCED_HISTORY
```

Expose fidelity in query results.

---

# 319. History must not pollute hot graph semantics

The current graph remains optimized for current-state queries.

Do not turn ordinary:

```text
neighbors()
path()
impact()
```

into temporal queries implicitly.

Use explicit current vs historical operations.

Hot projections should not carry every historical version unless needed.

---

# 320. Historical derived knowledge cache

Frequently queried historical semantic results may be materialized.

Cache key includes:

```text
historical generation
interpretation mode
ontology/rule set
query
configuration
```

Retrospective current-semantics results invalidate when current ontology/rules change.

Historical-original results do not.

---

# 321. Eventual future visualizations

SyntaxMesh still does not own a visualization frontend.

But external consumers should be able to visualize:

```text
entity timeline
change DAG
consequence DAG
blast-radius evolution
migration phases
contract regression history
runtime activation timeline
```

through public DTOs/NDJSON.

The core provides data, not WebGL/UI.

---

# 322. What v5 specifically rejects

Do not implement:

```text
a "butterfly effect" boolean
a magical causality score
causal inference from chronology alone
full raw APM telemetry storage
full Git-host replacement
employee productivity scoring
opaque AI-generated historical narratives without evidence
one giant historical graph loaded into RAM
full replay of every Git commit for every query
```

The desired capability is evidence-backed temporal system intelligence.

---

# 323. v5 agentic-coding payoff

For agents, v5 changes the question from:

```text
"What does this code do now?"
```

to:

```text
"What is this?"
"Why does it exist?"
"What introduced it?"
"What depended on it later?"
"Is that reason still active?"
"When was it last used?"
"What happened the last time this invariant changed?"
"What consequences did a similar migration create?"
```

This is especially valuable for:

```text
legacy cleanup
large migrations
cross-repository refactors
incident fixes
security hardening
architecture modernization
deprecation removal
```

It reduces the chance that an agent deletes or rewrites something whose purpose is only visible historically.

---

# 324. v5 relationship to v4 Codex experiment

The Codex fork experiment should remain staged.

Baseline:

```text
upstream Codex
```

Then:

```text
Codex + current SyntaxMesh context
```

Then separately:

```text
Codex + history-aware SyntaxMesh context
```

This makes the incremental value of historical/consequence intelligence measurable.

High-value evaluation tasks should deliberately contain:

```text
old migration bridges
historical incident guards
deprecated-but-still-used interfaces
reintroduced regressions
cross-repo evolution
```

---

# 325. v5 product positioning extension

SyntaxMesh remains:

> **a local-first, incremental, provenance-backed source and system intelligence engine.**

v5 adds:

> **It also maintains temporal engineering intelligence: how system facts, concepts, contracts, architecture, runtime evidence, and engineering decisions evolved, and how evidence-backed consequences propagated across later generations.**

Do not position it as a generic causal-inference platform.

Its scope remains software and engineering systems.

---

# 326. v5 final architectural progression

The full conceptual pipeline is now:

```text
source/system/runtime/document evidence
                |
                v
        versioned canonical facts
                |
                v
         typed live graph
                |
                v
       ontology/concept model
                |
                v
     deterministic derivations
                |
                v
   contracts + truth maintenance
                |
                v
       semantic generations
                |
                v
       ChangeSet / ChangeEvent
                |
                v
   temporal consequence lineage
                |
                v
 historical / trajectory reasoning
                |
                v
 evidence-backed engineering decisions
```

The critical invariant is:

> **SyntaxMesh may discover that a small historical change had very large later consequences, but it reaches that conclusion by traversing explicit versioned facts, dependencies, derivations, runtime observations, and later change relationships—not by labeling temporal coincidence as causality.**

That is the v5 temporal intelligence model.

---

# 327. v5 final decision summary

```text
Historical graph state:                 FIRST-CLASS
Historical semantic state:              FIRST-CLASS
Docs/ADR/config/schema history:          FIRST-CLASS where ingested
Runtime evidence history:                SUMMARIZED / PROVIDER-BACKED
Logical ChangeSet identity:              YES
Atomic ChangeEvent identity:             YES
Valid-time vs observed-time distinction: YES
Change-to-change dependencies:           YES
Direct consequence lineage:              YES
Delayed consequence discovery:           YES
Cross-repository consequence lineage:    YES
Blast-radius history:                    YES
First-possible / first-observed split:    YES
Semantic origin / decision lineage:       YES
Predicted vs realized impact:             YES
Historical retrospective re-analysis:     YES, explicitly labeled
Long-range consequence discovery:        YES
"Butterfly effect" primitive:             NO
Causality from temporal adjacency:        NEVER
Historical explanation provenance:       REQUIRED
Hot current-state performance protected:  REQUIRED
SyntaxMesh owns transformations:          NO
Change Engine remains separate:           YES
Codex/history integration:                OPTIONAL / BENCHMARKED
```

The practical goal is:

```text
not merely:
    "show me what changed"

but:
    "show me how this system got here,
     which later things depended on this change,
     when its consequences appeared,
     and why SyntaxMesh believes those relationships exist."
```
