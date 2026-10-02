# Local operations runbook (v0)

This runbook covers the current local CLI and embedded stores. SyntaxMesh is
still at the first vertical slice; it does not yet provide a daemon, online
backup command, or snapshot import command. Keep one writer per store at a time.

## Build the AI documentation graph

Uncached inference reports aggregate `semantic_progress` counts on stderr: the
pending request/prompt plan, then validated and failed request counts as each
original prompt group finishes. Validation is not graph publication. Full cache
reuse emits no inference-progress lines. Progress contains no source text,
paths, credentials, endpoint, or model name, and stdout remains unchanged
([ADR-0144](adr/0144-host-only-semantic-progress.md)).

The shared v3 extraction policy asks for authored decisions and rationale,
preserves proposal/optional/rejected status, and permits jointly supported claims
only within one independently cached request. Source text/headings are untrusted
data, not instructions. Exact quote validation remains the acceptance boundary;
the prompt does not prove model entailment or injection resistance. The version
and actual prompt hash change cache identity, so older prompt completions are
retained but not silently reused as v3 output
([ADR-0150](adr/0150-source-grounded-rationale-and-joint-claim-prompt.md)).

With local Ollama running and the selected model installed, run from the source
repository root:

```sh
syntaxmesh index --semantic
```

The default model is `qwen3:latest`. SyntaxMesh indexes supported source and
documentation, discovers the local model's reported digest, reuses unchanged
document results through Penelope, and publishes validated semantic facts with
history retained. It checks the digest after inference and before publication;
a changed digest or unavailable metadata fails the semantic step and leaves
completed jobs reusable where their identities still match. It does not start
Ollama or download models.

Standard local Ollama endpoints (loopback port 11434) run one packed prompt
group at a time to reduce GPU pressure; other endpoints retain the four-worker
limit. This port heuristic does not identify arbitrary custom-port providers.
Packing, cached results, and cache identity are unchanged
([ADR-0151](adr/0151-conservative-local-ollama-inference-scheduling.md)).

Add `--semantic-cross-document` for a second, bounded joint-source layer in the
same command. Document output stays independently cached; compound requests
include at least two documents and cite original chunks, not generated claims.
Compound inputs are packed in normalized directory/path and source-span order
to favor neighboring documents; byte/chunk bounds can still split a document or
cross a directory boundary. Paths are not sent to the provider
([ADR-0148](adr/0148-directory-local-semantic-batch-partitioning.md)).
Both layers merge before one atomic semantic replacement. A compound failure
leaves completed document jobs reusable without publishing document-only output.
`batches` and `cache_reused` then count both document and compound cache units.
Changing one document recomputes it and affected compound inputs, not unchanged
document extraction. Extra cold inference is explicit; corpus-wide reasoning,
automatic aliases, and real-model quality are not implied
([ADR-0146](adr/0146-one-command-bounded-cross-document-enrichment.md)).

Source request construction uses a 32 KiB raw text/heading bound. Packed user
messages are capped at 48 KiB of their exact serialized UTF-8 JSON, including
escaping, hashes, heading arrays, and separators. A request that cannot fit is
rejected before inference rather than silently truncated. This cap excludes
the system message and is not a provider-specific token-window guarantee
([ADR-0147](adr/0147-exact-semantic-user-prompt-byte-bounds.md)).

Repeated unchanged enrichment does not add a generation, including after a
model revision has replaced the semantic layer. Inactive provenance remains
retained for history and does not count as a replacement difference; explicitly
clearing an already-empty semantic layer is also a no-op
([ADR-0140](adr/0140-semantic-replacement-provenance-idempotence.md)).

Identical normalized semantic assertions from separate documents coalesce while
retaining separate source support edges. Deleting one source and enriching again
rebinds shared concepts to surviving evidence; deleting all document sources
clears current semantic facts without deleting prior graph generations. Restored
unchanged documents can reuse cached output with the same provider identity.
File and verified-Turso CLI fixtures cover this lifecycle with the provider
offline after initial extraction. This is exact-assertion coalescing, not general
concept alias resolution or cross-document AI synthesis.

`semantic_usage` reports per-invocation provider token totals separately from
metadata calls, including completions rejected before publication. `missing`
and `invalid` distinguish unavailable reports from reported zero usage;
`overflow=true` marks totals `unknown`. Cached results add no inference usage.
These are provider reports, not monetary costs or durable billing history.

Transient inference failures retain the three-attempt cap. Integer-seconds or HTTP-date
`Retry-After` headers override the 250/500 ms backoff; requests for more than
five seconds stop the current step and leave its job retryable instead of
retrying prematurely. Past dates produce zero delay; missing/malformed headers use fallback
backoff. Authentication failures are not retried. No delayed-retry scheduler
is implied ([ADR-0132](adr/0132-provider-directed-semantic-retries.md),
[ADR-0133](adr/0133-http-date-semantic-retries.md)).

For cache-only semantic execution, add `--semantic-offline` and the same
`--semantic-model-revision <revision>` used for online enrichment. Keep provider,
model, endpoint, and credential configuration unchanged. This policy makes no
inference or model metadata HTTP calls, including for remote HTTPS providers,
and conflicts with `--allow-network`. Cache misses fail the semantic step with
retryable jobs; deterministic source indexing remains independent. No partial
semantic batch publishes. Deterministic reindexing may invalidate semantic facts
grounded in edited source; prior generations remain available in history.
Automatically discovered digests and asserted revisions
use distinct identities, so supply the asserted revision on the initial online
run if offline reuse is needed.

`provider_requests` counts inference attempts; `metadata_requests` counts model
revision checks. A cache hit can therefore make zero inference calls while
still checking metadata. Automatic discovery needs the local service on every
run. For providers without Ollama metadata, or explicit offline cache reuse,
supply `--semantic-model-revision <immutable-revision>` in the same command.
That is a caller assertion: update it whenever the provider/model changes.
Remote HTTPS still requires `--allow-network` and uses the explicitly configured
endpoint. See [ADR-0129](adr/0129-semantic-model-revision-identity.md) for the
revision-consistency and attestation limits.

## Evaluate a live semantic provider

Agent context packs use schema v2 and include `evidence_class` on each item.
Graph paths take their edge's class; other items take their selecting node's
class. A source snippet selected through an AI claim can therefore be marked
`SemanticInference` even though its bytes are hash-verified original source.
That distinction is intentional: verified quotes do not establish entailment.
Legacy items without the field deserialize as unknown (`null`), never as
`SourceFact` ([ADR-0153](adr/0153-provenance-classified-context-items.md)).

With an already-running provider, use this opt-in development command:

```sh
SYNTAXMESH_SEMANTIC_EVAL_ENDPOINT=http://127.0.0.1:11434/v1 cargo make evaluate-semantic-provider
```

Optional `SYNTAXMESH_SEMANTIC_EVAL_MODEL` selects the model (default
`qwen3:latest`). Providers without Ollama metadata require
`SYNTAXMESH_SEMANTIC_EVAL_REVISION`; remote HTTPS additionally requires
`SYNTAXMESH_SEMANTIC_EVAL_ALLOW_NETWORK=true` and may incur provider costs.
The normal `SYNTAXMESH_SEMANTIC_API_KEY` is supported but never recorded.

Set `SYNTAXMESH_SEMANTIC_EVAL_CROSS_DOCUMENT=true` to evaluate the existing
two-layer command on the same controlled corpus. Absent/false keeps document-only
evaluation; any other value is rejected before inference. Two-layer mode adds
cold inference work and expects three warm cache units instead of two.

Evidence is retained under `target/semantic-provider-results/run-*`: two
controlled documents, CLI logs, the durable snapshot, and `report.json`.
The command checks unchanged-document cache reuse and reports accepted labels,
structured triples, shared concept identities, producer versions, and exact
source quotations with byte ranges in `claim_review`. Review extraction checks
accepted file hashes and support provenance rather than trusting provider output.
Report v4 also records the selected mode, cold/warm end-to-end indexing durations
(including publication, not inference-only latency), and the number of claims
with verified quotations from multiple distinct source paths. A joint-source
count does not prove entailment or extraction quality. Both cold/warm raw logs
are retained ([ADR-0149](adr/0149-cross-document-semantic-evaluation-mode.md)).
The evaluator also appends one paragraph to its generated ownership document,
then repeats the edited input unchanged. It requires reuse of the unchanged
document, positive edit inference, preserved initial history, and no inference
or new generation on the edited repeat. Original bytes remain under
`initial-source/`; edited bytes remain under `source/`. Both edit invocations'
logs, timings, edited hash, and `edited_claim_review` are retained. This adds
one document request plus an affected compound request in cross-document mode;
bounded retries may add HTTP calls and remote provider costs
([ADR-0152](adr/0152-semantic-evaluator-sparse-edit-workload.md)).
Label presence is not a semantic-quality score; inspect
claims and source evidence, especially when no claims were accepted. CI tests
the harness with a mock but does not run the live provider. See
[ADR-0138](adr/0138-live-semantic-evaluation-command.md) and
[ADR-0139](adr/0139-semantic-evaluation-source-evidence.md).

## What is stored and what can be rebuilt

- The source repository is the rebuild source for supported Rust, Python,
  TypeScript, and JavaScript source facts.
- A `FileGraphStore` snapshot is a restartable reference/conformance store. It
  is a binary serialized snapshot written to a unique sibling temporary file,
  synchronized, and atomically renamed; it is not the Turso production schema
  and does not promise crash durability or coordinated multi-writer safety.
- The local Turso database is the canonical embedded store for this slice. It
  uses WAL mode; open validates indexed projections and streams canonical facts
  to check the graph root without restoring a graph-sized heap snapshot.
- `export` and `export-turso` produce NDJSON for inspection/interchange. There
  is no import command, so an export is not a backup that can restore a store.
- Re-indexing source does not recreate runtime observations or arbitrary
  extension facts. Preserve/replay the inputs from those producers separately.

The current CLI indexes `.rs`, `.py`, `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`,
`.cjs`, `.sh`, `.bash`, `.md`, `.markdown`, `.txt`, `.text`, `.rst`, `.adoc`,
and `.asciidoc` files. Bash and documentation extraction is structural and
runs in Rust without executing source files. Local Markdown links resolve only
to indexed local documents; network and heading-anchor resolution are not
attempted. `--verify`
enables optional StateChronicle verification for each
publication; it does not change the graph source of truth or make an NDJSON
export restorable.

## Check a store

Use the command matching the backend:

```sh
cargo run -p syntaxmesh-cli -- status /path/to/syntaxmesh.snapshot
cargo run -p syntaxmesh-cli -- status-turso /path/to/syntaxmesh.db
```

To compare indexed supported-language file versions against the current source
tree, add the exact source root used for indexing:

```sh
cargo run -p syntaxmesh-cli -- status /path/to/syntaxmesh.snapshot /path/to/source
cargo run -p syntaxmesh-cli -- status-turso /path/to/syntaxmesh.db /path/to/source
```

With a source root, status verifies that its root-derived repository/worktree
scope matches the store and reports unindexed, changed, and removed supported
source files.
It exits non-zero for stale source. Without a source root it reports
`index_freshness=not_checked`; the engine API likewise requires callers to pass
the inventory from their own runtime's scanner. This comparison does not track
runtime observations or arbitrary extension facts.

To inspect terminal indexing rejections without triggering recovery, request a
bounded page (up to 100 items) and pass the returned run-ID cursor to continue:

```sh
cargo run -p syntaxmesh-cli -- workflow-rejections /path/to/syntaxmesh.snapshot 25
cargo run -p syntaxmesh-cli -- workflow-rejections-turso /path/to/syntaxmesh.db 25
```

Pages are ordered by run ID, not wall-clock time. Current records report typed
stale-base context (expected and actual generations); rejected records written
by older versions are reported as `unknown_legacy`. These commands only read
the Penelope journal and do not recover or alter operations.
The pages are memory-bounded; because the shared journal is not separately
indexed by phase, a sparse page may scan more run keys to find its results.

Opening a Turso store validates indexed projections and the graph root. The
status command then reports the current generation, verification status, logical graph-root
and reference checks, fact counts, and Penelope workflow counts. It exits
non-zero if a loaded generation fails either logical check. Any open, decode,
restore, or workflow-diagnostic error is also a failed check; preserve the files
and error output for investigation.

For a readable canonical-state export, redirect stdout to a new file (do not
overwrite the only backup):

```sh
cargo run -p syntaxmesh-cli -- export /path/to/syntaxmesh.snapshot > /path/to/graph.ndjson
cargo run -p syntaxmesh-cli -- export-turso /path/to/syntaxmesh.db > /path/to/graph.ndjson
```

Successful export verifies that the current query/export path can read the
generation. It does not independently verify a FileGraphStore graph root or
provide a restore mechanism.

For an explicit backend integrity check, run the full check separately from
routine status:

```sh
cargo run -p syntaxmesh-cli -- integrity /path/to/syntaxmesh.snapshot
cargo run -p syntaxmesh-cli -- integrity-turso /path/to/syntaxmesh.db
```

The SQL backends run their full read-only physical integrity scan and validate
each retained graph checkpoint against its history manifest and persistent
historical root, reporting every finding; a result other than a single `ok` is
a failure. This is intentionally an explicit operator check, not an open-time
or query-time cost, and may take longer than `status` on a large database. A
corrupt checkpoint does not affect root-backed historical reads, but it is
reported because checkpoints are retained for recovery and rebuild. The
snapshot command checks that the file can be decoded and matches the open
in-memory state; it is not a physical page audit. Neither operation checks
source freshness, crash durability, nor the recoverability of runtime or
extension inputs.

StateChronicle's optional generation chain has a separate deep audit. Normal
verified publication validates the latest chain head and links the new record
to it; it does not replay every older record. Run the explicit full-chain
audit when checking retained history for earlier corruption:

```sh
cargo run -p syntaxmesh-cli -- statechronicle-verify /path/to/syntaxmesh.snapshot
cargo run -p syntaxmesh-cli -- statechronicle-verify-turso /path/to/syntaxmesh.db
```

The command reports `not_recorded` for stores with no StateChronicle history.
This audit is read-only and its cost grows with retained history depth.

## Start and inspect verified history

Verification remains opt-in. To persist the choice for this source project,
create `syntaxmesh.toml` at the source root with:

```toml
[history]
verified = true
```

Or have the CLI create that file without overwriting an existing config:

```sh
cargo run -p syntaxmesh-cli -- init /path/to/source --verified-history
```

With that policy, each indexing publication uses StateChronicle by default.
Without a config (or with `verified = false`), indexing remains durable-only.
The per-run `--verify` flag can force verification on when the project setting
is false. There is no per-run disable flag because skipping a generation would
prevent this chain from being extended later.

ECMAScript module resolution is a separate opt-in. To use the CLI's Oxc-backed
Node profile (including TypeScript config path mappings), add:

```toml
[module_resolution]
profile = "node"
```

or initialize a new project with `syntaxmesh init --node-module-resolution`.
Static/dynamic imports and re-exports use Node `import` conditions;
`require()` occurrences use Node `require` conditions. Resolution edges are
created only when the target module is in the SyntaxMesh source index. This
setting does not enable StateChronicle verification. Other hosts should inject
their own runtime-appropriate `ModuleResolutionProvider` into the Engine.
Python module resolution is a separate opt-in and supports static source
modules under classic `__init__.py` packages. Use
`[module_resolution] profile = "python"` or
`syntaxmesh init --python-module-resolution`. To enable both resolvers, use
`profiles = ["node", "python"]`; optional `source_roots = ["src", "lib"]`
are searched in order, relative to the indexed repository. This resolver does
not execute Python or inspect installed packages, namespace packages, zip
imports, or import hooks. See [ADR-0072](adr/0072-python-module-resolution-profile.md).
For the file/reference store:

```sh
cargo run -p syntaxmesh-cli -- index /path/to/source /path/to/syntaxmesh.snapshot
cargo run -p syntaxmesh-cli -- statechronicle-verify /path/to/syntaxmesh.snapshot
cargo run -p syntaxmesh-cli -- status /path/to/syntaxmesh.snapshot /path/to/source
```

For Turso, explicitly migrate before the first open, then use the Turso host
commands:

```sh
cargo run -p syntaxmesh-cli -- turso-migrate /path/to/syntaxmesh.db
cargo run -p syntaxmesh-cli -- index-turso /path/to/source /path/to/syntaxmesh.db
cargo run -p syntaxmesh-cli -- statechronicle-verify-turso /path/to/syntaxmesh.db
cargo run -p syntaxmesh-cli -- status-turso /path/to/syntaxmesh.db /path/to/source
```

The index command reports `Verified` when the opted-in publication and
StateChronicle append succeed; status retains that state after reopening. If
verification is first enabled on an existing store, the current generation is
an explicit anchor: earlier graph generations are not retroactively verified.
Keep the project setting enabled for each generation intended to join the
verified chain. If verification is disabled for an intermediate generation,
start a new store for another verified chain rather than trying to extend the
old one across the gap.
This local hash-linked history detects malformed or altered retained records,
but is not a signature, external timestamp, or portable proof. The explicit
full-chain command replays all retained verification records; its work grows
with verification-history depth, while ordinary publication checks only the
current chain head.

## SQLite schema migrations

The SQLite adapter follows an explicit-migration lifecycle. `open` does not
create a missing database, apply an upgrade, create an index, or repair a
derived history projection. Before embedding the backend, run
`SqliteGraphStore::migrate(path)` during the host's controlled setup/upgrade
phase, then open it with `SqliteGraphStore::open(path)`. Migration bootstraps a
new store or applies each pending registered step transactionally; a stale or
incompatible store fails closed at open. Keep the database backup and migration
error output if an upgrade fails, and retry only after diagnosis—the runner
resumes from the last committed migration.

The adapter rejects a database path whose final component is a symlink or not
a regular file, and opens SQLite with `NOFOLLOW`. Connections also enable
defensive mode, foreign-key enforcement, trusted-schema-off, and page-cell
checks. This follows the local SQLite hardening used by Shardline while keeping
path ownership and setup under the embedding host's control.

The ordered version/name/checksum registry lives in
`crates/syntaxmesh-store-sqlite/src/backend/migrations.rs`; each registered
transition points to one transaction-owning upgrade function in the SQLite
adapter. Schema-only SQL is checked in under that crate's `migrations/`
directory and validated against the registry by the Rust `syntaxmesh-xtask`
(`cargo make sqlite-migrations`).
The additive v13→v14 consequence-schema step and v14→v15 acceptance-prefix
projection have registered down scripts whose checksums cover both directions;
tests execute the up/down round trip and real upgrade fixtures. This follows Shardline's explicit up/down migration
manifest for schema-only work. Historical graph/data upgrades stay in Rust
because they validate and transform canonical encoded graph records and remain
forward-only where reversal would risk data loss.

The CLI exposes `sqlite-migration-status` as a read-only registry/ledger
inspection and `sqlite-migrate` as the explicit upgrade operation:

```sh
cargo run -p syntaxmesh-cli -- sqlite-migration-status /path/to/syntaxmesh.db
cargo run -p syntaxmesh-cli -- sqlite-migrate /path/to/syntaxmesh.db
```

## Read-only MCP host

Migrate and index the Turso database before starting the MCP process. The host
does not migrate, index, or mutate graph state:

```sh
cargo run -p syntaxmesh-cli -- turso-migrate /path/to/syntaxmesh.db
cargo run -p syntaxmesh-cli -- index-turso /path/to/source /path/to/syntaxmesh.db
cargo run -p syntaxmesh-mcp -- /path/to/syntaxmesh.db /path/to/source cl100k_base
```

The stdio server exposes `status`, `search`, `node`, `neighbors`, bounded
directed `path`, and `context` tools. `context` requires a positive token budget
and returns a generation-pinned evidence pack measured with the configured
`cl100k_base` or `o200k_base` tokenizer. The source root is canonicalized at
startup; context reads only indexed repository-relative paths that resolve
inside that root, and the query layer rejects source bytes whose hash differs
from the pinned generation. The process pins the latest generation visible at
startup; restart it after indexing to query a newer generation. This remains a
read-only host and does not push refresh notifications or expose graph mutation.

For historical context, add optional `generation` (64 hexadecimal characters)
to the same `context` tool arguments, for example
`{"query":"caller","token_budget":4096,"generation":"<generation-hex>"}`.
The selected generation must exist and belong to the host's repository/worktree.
Omitting it preserves startup-generation behavior. Historical selection does
not bypass freshness checks: restart a superseded host before any query.
Historical graph reads do not replay indexing or invoke AI. The host still reads
current filesystem bytes, not an archive; changed source hashes produce
`stale_source` warnings and omit snippets. Substring search still scans the
selected generation's node names, and high-degree adjacency work is not yet
independently budgeted.
Historical context merges adjacency with at most two buffered edge pages (one
per direction). This does not bound extension-payload bytes, selected parallel
edges, or total scanning work; current-only context still uses adjacency vectors.

For a retained generation, `neighbors-at` and `neighbors-at-turso` return one
bounded incoming or outgoing adjacency page as temporal NDJSON. Supply the
generation and node IDs from a known graph record; the footer's typed
`next_cursor.after_edge` is passed as the optional final CLI argument to fetch
the next page. The cursor is bound to that generation, endpoint, and direction.
Page limits must be in `1..=1000`; the output footer states whether more edges
remain. Example:

```sh
cargo run -p syntaxmesh-cli -- neighbors-at /path/to/graph.snapshot <generation-hex> <node-hex> outgoing 100
cargo run -p syntaxmesh-cli -- neighbors-at-turso /path/to/syntaxmesh.db <generation-hex> <node-hex> incoming 100
```

To continue, pass the `after_edge` value from the returned `next_cursor` as
the final argument; do not reuse it for another generation, node, or direction.

These commands read the retained generation through its persistent incidence
root; they do not reconstruct it by replaying every earlier request/delta or
materialize the complete historical graph. Reference File/InMemory stores may
derive the answer from retained history. For a bounded weak neighborhood, use
the `neighborhood-at[-turso]` pair. It emits a finite node/edge NDJSON stream
with BFS depths and a footer reporting scanned incidences, serialized item
bytes, and truncation. Hops, nodes, edges, scanned incidences, and serialized
result bytes are independent bounds; the result-byte cap is at most 16 MiB:

```sh
cargo run -p syntaxmesh-cli -- neighborhood-at /path/to/graph.snapshot <generation-hex> <seed-node-hex[,seed-node-hex...]> 4 1000 2000 10000 16777216
cargo run -p syntaxmesh-cli -- neighborhood-at-turso /path/to/syntaxmesh.db <generation-hex> <seed-node-hex> 4 1000 2000 10000 16777216
```

The query is exact-generation only. It has explicit upper bounds for hops,
nodes, edges, scanned incidence entries, and serialized result bytes. Exceeding
an output/work budget is visible in the footer (or rejected if mandatory seed
records cannot fit). These are bounded context queries, not
time-respecting traversals, calendar-range reconstruction, relation/evidence
filtering, or causal/transitive analysis.

Status reports missing/empty databases as uninitialized with all registered
steps pending; it does not create a file or schema. For legacy schemas predating
the migration ledger it reports that the ledger is not available. Current
schemas validate the ordered ledger and SQL checksums. There is intentionally no
generic `down` command: historical Rust transforms have no inverse, and the
v13→v14 down script drops authored consequence data. Keep a backup and treat it
as a fixture/rollback test utility only. Hosts embedding the SQLite crate can
still call the Rust API directly:

```text
use syntaxmesh_store_sqlite::SqliteGraphStore;
use syntaxmesh_store::StoreError;

fn open_sqlite(path: &std::path::Path) -> Result<SqliteGraphStore, StoreError> {
    SqliteGraphStore::migrate(path)?;
    SqliteGraphStore::open(path)
}
```

This is an API-shape example; the CLI commands use the same explicit lifecycle.
Fresh databases must follow the same explicit bootstrap call.

## Cooperating CLI writer ownership

For sources without reliable native filesystem events, use explicit polling:

```sh
syntaxmesh watch /path/to/source /path/to/state/graph.snapshot --poll-only --reconcile-ms 1000 --verify
```

This rescans inventory every second after completed indexing, reusing unchanged
fingerprints to avoid new generations. It is not constant-time polling: source
files are read each interval. Keep state outside the source root. Use watch-turso
only with an explicitly migrated database and without separate active Turso
readers. Native mode does not silently downgrade on errors; restart with the
explicit flag when appropriate.

`watch`, `watch-turso`, `index`, `index-turso`, `ingest-extension`, `ingest-extension-turso`,
`sqlite-migrate`, and `turso-migrate` share an exclusive advisory lease on
`<resolved-store-filename>.syntaxmesh-owner.lock`. Existing store symlinks are
resolved; a new target uses its canonical parent. Competing commands fail with
`already owned by another cooperating writer` before opening the store.

Wait for the current command to finish, or stop that process normally, then
retry. The operating system releases the lock when its file handle closes or
the process exits. The sidecar remains intentionally: its presence alone does
not indicate a live owner. Do not delete or replace it to clear contention;
another process could then acquire a different file and bypass the live owner.
No stale PID cleanup is needed for this lease.

Store targets must be regular files or genuinely absent filenames. Directory
targets and dangling store symlinks are rejected before sidecar creation; point
the command at the intended destination instead. Existing regular-file symlinks
remain supported. Sidecars must themselves be regular files, not symlinks or
directories ([ADR-0186](adr/0186-cli-ownership-target-validation.md)).

This coordinates only these CLI commands. Embedded writers must coordinate
separately; hard-link aliases and shared/network filesystems are not covered.
It is not complete daemon ownership, a recovery operation, or a hot-backup
mechanism. Read-only commands do not acquire this exclusive lease. Extension
grant/frame rejection happens before acquisition and cannot create a store.
See [ADR-0184](adr/0184-cli-index-ownership-lease.md) and
[ADR-0185](adr/0185-cli-publication-and-migration-ownership.md).

## Make a cold local backup

1. Stop all processes that can write the store; do not copy a live Turso WAL
   database.
2. Create a new, empty backup directory outside the active store directory.
3. For a FileGraphStore, copy the snapshot file. If a sibling name matching
   `<snapshot>.tmp-*` is present, preserve it too for diagnosis; do not
   substitute it for the snapshot.
4. For SQLite or Turso, copy the main database and any sibling `-wal` and
   `-shm` files together, preserving their names. Treat this set as one cold
   backup. If the exact database file set is unclear, preserve the whole
   containing directory rather than selecting files by guesswork.
5. Keep the source checkout and any extension/runtime producer inputs needed
   to recreate facts that source indexing cannot regenerate.
6. Reopen/check the copied store using the commands above, pointing them at the
   copied paths. Keep the original untouched until this check succeeds.

The backend-conformance test `cold_database_copies_restore_current_and_historical_graphs`
exercises this cold-copy boundary for SQLite and Turso: it closes the source,
copies the database plus any WAL/SHM sidecars, then checks physical integrity
and compares both the current and prior-generation graph after opening the
copy; it also checks a durable workflow record survives the copy. This is a
recovery regression test, not a hot-backup API or a crash/power-loss recovery
guarantee.

The v0 CLI has no backup/snapshot utility, coordinated hot backup, checksum
report, or import command. For a long-lived or production-like store, stopping
writers and retaining the database files together is required; do not treat
plain NDJSON as a database backup.

## Rebuild after corruption or a failed migration

1. Stop writers and preserve the entire affected store directory/files and the
   exact error output. Do not delete, truncate, or overwrite the damaged store
   while diagnosing it.
2. Create a fresh store at a different path. Re-index the original source root:

   ```sh
   cargo run -p syntaxmesh-cli -- index /path/to/source /path/to/rebuilt.snapshot
   cargo run -p syntaxmesh-cli -- status /path/to/rebuilt.snapshot
   cargo run -p syntaxmesh-cli -- export /path/to/rebuilt.snapshot > /path/to/rebuilt.ndjson
   ```

   For the Turso backend, substitute `index-turso`, `status-turso`, and
   `export-turso`, using a new database path. Add `--verify` to the index
   command only if StateChronicle verification is wanted.
3. Compare the rebuilt generation, file/node/edge/provenance counts, and
   representative queries with the last known good status/export. Counts alone
   are not a semantic equivalence proof.
4. Replay runtime observations and extension-produced facts from their owning
   producers; the current CLI cannot re-import them from NDJSON.
5. Switch the host to the rebuilt path only after the rebuilt store opens and
   representative queries succeed. Retain the damaged store and backup until
   the replacement has been exercised.

Opening an unsupported schema or failing Turso graph-root restoration is
fail-closed for that open attempt. The current v0 recovery action is to preserve
the failed store and rebuild to a new path; schema repair, automatic restore,
and a backend-wide integrity report are not implemented.

## Turso schema migrations

Turso follows the same explicit lifecycle as SQLite. `open` rejects a missing
or stale schema and validates WAL mode, the migration ledger, indexed
projections, generation history, and graph root; it never migrates, creates
indexes, or backfills history. During controlled setup or upgrade, run:

```sh
cargo run -p syntaxmesh-cli -- turso-migration-status /path/to/syntaxmesh.db
cargo run -p syntaxmesh-cli -- turso-migrate /path/to/syntaxmesh.db
```

Status is read-only and reports the registered version/name path. Schema
transitions and their version advancement are transactional. Version 17 adopts
the pre-ledger history with unknown timestamps; SQL-backed migrations store a
BLAKE3 checksum, while Rust payload transforms are covered by migration
fixtures. There is no operator-facing downgrade command. Make a cold backup
before upgrading a valuable local database; if migration fails, preserve the
database and diagnose before retrying. The FileGraphStore binary snapshot is a
reference format, not a stable cross-version interchange format.

## Current operational gaps

- No persistent index-lag or freshness metric, backup command, or database import.
- Complete-generation status/export materialize the requested graph output and
  may not scale to large repositories; ordinary startup validation streams
  indexed facts and does not retain a graph-sized heap snapshot.
- New completed Penelope rows are compact receipts; exact retries CAS-compact completed v1-v4 rows while preserving their historical process-definition digest. Existing records are not rewritten at startup, so unretried full-payload rows may remain until an explicit maintenance operation exists.
- Evidence-backed consequence assertions can be atomically published and read
  at a pinned generation. The embedded query API supports cursor-paged exact
  endpoint reads and deterministic bounded weakly connected traversal with
  explicit hop, endpoint, edge, and scanned-incidence bounds; see
  [ADR-0043](adr/0043-indexed-consequence-endpoint-reads.md) and
  [ADR-0044](adr/0044-bounded-consequence-neighborhood.md) and
  [ADR-0092](adr/0092-bounded-consequence-neighborhood-work.md). Traversal
  retains edge direction while allowing either-direction connectivity, reports
  scanned incidence counts, and marks work/output truncation. The
  `consequence-neighborhood[-turso]` CLI requires a
  `<max-scanned-incidences>` argument and emits temporal NDJSON schema v10;
  see [ADR-0045](adr/0045-consequence-neighborhood-ndjson.md).
- Historical adjacency has a separate generation-pinned page contract in
  temporal NDJSON schema v10, exposed by `neighbors-at[-turso]`; see
  [ADR-0086](adr/0086-generation-pinned-historical-neighbor-pages.md). Fixed
  page/continuation equivalence is covered across File and Turso CLI and direct
  SQLite/Turso store tests. The temporal benchmark measures fixed-page latency
  and (when run with its opt-in instrumentation) tree-cache lookups, incidence
  page loads, and edge-payload fetch counts against retained-history depth and
  unrelated graph size; see [ADR-0090](adr/0090-benchmark-historical-index-read-counts.md)
  and [ADR-0091](adr/0091-resumable-persistent-adjacency-reads.md). SQL
  statement counts and representative workload scaling remain open.
  [ADR-0087](adr/0087-bounded-historical-neighborhood.md)
  adds bounded generation-pinned weak-neighborhood traversal to the Rust query
  API, composing historical pages with explicit hop/node/edge/scanned-incidence
  limits and truncation reporting. [ADR-0088](adr/0088-historical-neighborhood-stream-and-cli.md)
  adds a separate versioned node/edge/footer stream and paired File/Turso CLI
  commands. Generation/calendar-range selection, relation/evidence filters,
  and broad transitive propagation remain open.
- Extension panics/timeouts are not isolated by the in-process extension API.
- On Linux, `cargo make benchmark-file-writes` attributes successful
  write-family syscalls to the real-repository benchmark's SQLite/Turso
  database, WAL, and SHM files. It is a diagnostic only: tracing distorts
  timings, mmap dirty-page writeback is excluded, and these bytes do not
  represent total physical I/O or complete write amplification.
