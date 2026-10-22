# ADR-0349: Incremental inference stream checkpoints

Status: explicit schema, atomic baseline initialization, direct successor
synchronization and public retained reads implemented; persistent active-set
roots are implemented under ADR-0350; Penelope coordination remains pending.

## Decision

A complete rebuild receipt must never be reused to publish a witness delta.
Introduce a separate downstream stream rooted in an immutable complete selection.
Stream identity derives from its baseline operation; configuration and canonical
scope are fixed by that receipt. Execution controls and destination are not
semantic identity. Explicit schema v4 migration creates stream tables without
inventing streams or backfilling evaluated history.

`InferenceMaterializer::begin_stream` verifies and boundedly copies the baseline's
claims and ordered exact-premise postings into independent stream storage in one
SQLite transaction. The immutable `StreamOrigin` receipt identifies that root.
Repeated initialization validates retained baseline rows and must not reset later
stream coverage. Count, payload-byte and premise-posting admission apply before
publication, and any failure rolls back all stream tables. Existing standalone
materialization/lifecycle tables and receipts remain unchanged.

Each subsequent synchronization effect must accept exactly one immediate
canonical successor, with unchanged rule configuration. It atomically closes
affected active claims through indexed exact-premise postings, stores closure
reasons, inserts seeded new witnesses and publishes a retained checkpoint plus
the stream head. Claim payload generation remains its evaluated birth selection;
unchanged witnesses retain their earlier identity and are not copied/reidentified
at every generation. A restored canonical identity has a new exact version and
therefore a new witness lifetime.

Do not jump from one generation to a distant target using only target-active
openings: that would lose witnesses enabled and closed in intervening generations.
Catch-up must process accepted successors in order with bounded work. Each
checkpoint is an atomic effect; a multi-generation coordinator must explicitly
report partial progress and use Penelope journals before each effect. Recovery
validates the exact retained checkpoint, even after later head advancement.

Retained reads select stream claim intervals through indexed SQL and explicit
accepted checkpoint coordinates. They do not replay canonical history or rules,
and do not assert validity beyond checked coverage. Query cost includes index
seeks and requested output, not a blanket O(1) promise. Recursive inference,
contradictions and ontology/configuration migration remain separate layers.

`StreamSyncRequest` binds the immutable origin, one target, unchanged rules and
explicit inference/range/history/output/posting/evidence admission.
`sync_stream` returns a `StreamCheckpoint` with scope, accepted sequence,
predecessor and added/affected/reason/active counts. Exact completed target
retries return retained receipts without replay or head regression, after rule
configuration/scope and output-count admission. `stream_checkpoint` is a
read-only retained receipt lookup without canonical replay. A first implementation
may use two separately bounded canonical feed passes for closure and discovery;
it must not claim a single-pass feed or an O(1) synchronization cost.

`stream_claims` returns `StreamClaims { checkpoint, claims }` for an explicitly
retained checkpoint, with independent claim-count, decoded payload-byte and
ordered premise-posting admission. Individual claim payloads retain their birth
generation; the enclosing checkpoint identifies the queried selection. Pin all
receipt, interval and posting reads in one deferred SQLite transaction. Validate
birth/closure checkpoint coordinates and exact postings, reject partial or
excess active output, and never require a canonical store or rule evaluator.
The initial interval index may examine historical intervals; it is not the final
history-depth-independent active-set projection. Persistent checkpoint roots
remain a performance requirement rather than an O(1) claim for this SQL reader.

## Required evidence

Verify baseline count/byte/posting bounds, exact payload/posting copies,
idempotence, reopen, tamper rejection and transaction rollback. Upgrade from v3
must leave streams empty and roll back on interrupted ledger publication.
Subsequent synchronization needs complete-evaluation witness oracles at every
checkpoint, transient create/close/restore lifetimes, indexed posting plans,
exhaustion rollback and File/Turso Penelope interruption recovery. Baseline
initialization alone is not completion of incremental synchronization.

## Initial verification

Ten storage tests pass, including two stream fixtures and explicit interrupted
v3→v4 migration rollback. Baseline fixtures verify count/byte/posting admission,
atomic multi-table rollback, retry, reopen, empty scope and corrupted posting or
checkpoint rejection. A simulated later checkpoint checks that initialization
does not rewind coverage; it is not successor synchronization evidence. Strict
storage Clippy, architecture and the four-entry migration registry gate pass.
Verification for this slice is targeted, not a new completed full CI run.

## Direct successor synchronization evidence

`sync_stream` now combines indexed active-witness closure, exact reasons, seeded
new witnesses, retained checkpoint publication and head advancement in one
immediate transaction. `stream_checkpoint` validates root/scope, predecessor
sequence and active-count conservation without canonical replay. Historical
effect retries check canonical target coordinates and return the exact retained
receipt without rewinding coverage or re-evaluating rules.

Thirteen storage tests pass. Three synchronization fixtures compare active
witnesses with complete evaluation at every create/close/restore/empty checkpoint,
including retained reads after reopen and a transient witness that distant-target
catch-up would miss. Nine independent admission failures preserve prior coverage.
A mixed two-premise replacement closes and discovers atomically; an injected
checkpoint SQL failure rolls back both effects, postings and reasons. Additional
checks reject changed rules, corrupt matched payloads and inconsistent checkpoint
counts, and EXPLAIN confirms the actual query seeks the stream premise index.
These direct synchronization fixtures use the reference canonical backend;
durable File/Turso synchronization/recovery coverage remains required.

`stream_claims` now provides pinned public interval reads. Fifteen storage tests
pass, including comparison with the independent interval oracle across every
create/close/restore/empty checkpoint, exact byte/posting boundaries, missing or
excess posting rejection, corrupted payload/birth coordinates, empty selection
and foreign root rejection. Reopened reads work after dropping the canonical
store, proving that no source history or rule replay is required. Aggregate
checkpoint selection and immutable claim birth generations remain distinct.

ADR-0350 subsequently replaces active-ID interval scanning with persistent
checkpoint roots and exact payload seeks. Penelope-journaled stream workflows
remain open. A complete v0–v5 release is not implied.
