# ADR-0326: Generation-scoped source processing coverage

Status: proposed; no partial publication implementation yet.

## Evidence and reuse



Shardline's `store_files_partial_failure_rollback` test in
`crates/shardline-index/src/hub_local_sqlite.rs` performs an empty second write,
not an injected partial failure. Its name is not evidence of failure atomicity.
Retain SyntaxMesh's stronger mixed-update failure/repair fixtures.

## Intended contract

Keep strict ingestion the default. An explicit partial policy may tolerate only
`ExtractorError::SyntaxError`; unsupported input, parser setup/cancellation,
producer mismatch, invalid facts, configuration, resolver, storage and integrity
errors remain fatal. The policy belongs to the Engine/index preparation path,
not a host-specific error-swallowing loop.

Represent processing coverage as typed, generation-scoped canonical graph facts,
using the existing file, provenance and node history machinery. Append any new
`NodeKind` variant without changing persisted enum ordinals. Do not disguise an
extraction failure as a module-resolution diagnostic or a successful declaration.
Specify and test its identity and encoding before adding the public variant.

Use the existing `NodeKind::External` envelope for the initial SDK-owned type:
namespace `syntaxmesh.source-processing`, kinds `completed.v1` and
`syntax_failed.v1`. This adds no persisted core enum ordinal. The stable node ID
is derived from that namespace and file ID alone; status, hash and producer edits
are versions of one processing fact. Its source spans the observed file, retaining
the content hash. Processing provenance uses namespace
`syntaxmesh.source-processing.<attempted-namespace>` and the attempted producer
version, deliberately distinct from successful extractor provenance. Provenance
identity includes source hash, producer identity, status and diagnostic bytes.
Failures carry the bounded diagnostic envelope; completion carries no payload.
This typed builder is not proof that any Engine has published coverage.

The first SDK building block is `SyntaxDiagnostic`: at most 4,096 UTF-8 bytes,
retaining a prefix at a character boundary and an explicit truncation flag.
Its fields are private, with read-only accessors; constructing it cannot admit
an oversized diagnostic. This bounds the retained message, not parser allocation
or total publication size. It is not yet a persisted coverage encoding.

Its diagnostic-only payload uses the existing `ExtensionPayload` contract:
namespace `syntaxmesh.syntax-diagnostic`, schema 1, one truncation byte (0/1)
followed by the retained UTF-8 message. Decode rejects other flag values,
oversized messages, invalid UTF-8 and unsupported schemas in that namespace.
Missing/unrelated payloads mean no diagnostic, not processing success. This is
a binary fact payload, not NDJSON, and does not yet encode processing coverage.

Each attempted file records its actual content hash, attempted extractor namespace
and version, and explicit success or syntax-failure status. A failure carries a
bounded diagnostic with an explicit truncation flag. The containing generation
provides the temporal scope; do not put generation IDs into stable fact IDs.
Use a separately identifiable processing-evidence producer so the current
hash-plus-extractor-provenance shortcut cannot mistake a failed attempt for
successful extraction.

For a tolerated failure, publish the observed file version and processing failure
together with removal of its prior file-owned extracted facts. Remove dependent
resolution edges using the existing delta repair path. Old successful facts
remain available in earlier generations, never silently as current truth.
Successful files and failure coverage commit as one ordinary Engine/Penelope
publication; StateChronicle verifies that generation when enabled. Add no separate
failure journal, database connection or NDJSON communication/storage path.

Unchanged failed bytes must remain retryable. Producer changes also retry them.
A repaired source replaces failure coverage with successful coverage and facts;
a removed source removes its current coverage. Reprocessing an unchanged failure
must not fabricate success or create a new generation merely because an attempt
occurred. Generation acceptance time is not an extraction-attempt audit log.

Inventory/freshness reporting must distinguish observed-but-failed files from
successfully processed files. A complete input inventory or matching content hash
alone must not report complete processing. Queries and context consumers must be
able to expose incomplete coverage rather than imply complete repository truth.

## Required verification before availability


This ADR does not claim any of these new contracts or gates are implemented.

The preparation API starts with `SourceSyntaxPolicy::{Strict, RecordFailures}`
on Indexer, defaulting to Strict. RecordFailures adds completed processing facts
for newly extracted successful files and failed processing facts for syntax
rejections only. It is not exposed by Engine/CLI/daemon yet: legacy coverage
backfill, repeated-failure no-op semantics, persisted validation and host source
planning still require integration before end-to-end availability.

## First implementation evidence

The SDK diagnostic building block is implemented in focused `diagnostic.rs` and
child `diagnostic/tests.rs`; `lib.rs` contains declarations/re-exports only.
All 12 SDK tests pass, including exact-byte-limit, empty, ASCII overflow and
two/three/four-byte Unicode boundary cases. Strict all-target SDK Clippy and
workspace all-target/all-feature locked checking pass. No dependencies or lint
exceptions were added. Full contributor CI for this addition passes (exit status
0) in 199.14 seconds, including all 17 backend conformance scenarios (41.35
seconds) and generated API documentation. None of the new durable coverage gates
above is proved by these SDK tests or existing conformance scenarios.

The diagnostic-only payload codec now follows the existing source-role and
reference-resolution envelope pattern, without new dependencies. All 14 SDK
tests and focused strict Clippy pass. Tests cover round trips, explicit
truncation, exact byte encoding, invalid UTF-8, oversize, unknown flags/schemas
and absent/unrelated metadata. Full CI for this codec extension is not yet run.
No coverage node, Engine policy or persisted failed-file publication exists yet.

The SDK now constructs processing nodes/provenance through `SourceProcessing`
without changing core DTOs or adding dependencies. All 15 SDK tests and strict
all-target SDK Clippy pass. The identity regression checks status/hash/producer
versions retain one node ID, different files do not, failure provenance is distinct
from the attempted extractor, and diagnostics round-trip. These are builder
tests only: Engine publication, persisted decoding/validation and host reporting
are still open. Full contributor CI has not yet run for this builder extension.

Indexer now has the explicit preparation policy above. All 17 Indexer tests and
strict all-target Indexer Clippy pass. New in-memory tests verify strict mixed
batch rejection without publication, and opt-in valid-to-invalid retraction,
valid sibling publication, retained historical facts and repair replacing failed
coverage. No lint exceptions were added. This is not durable backend/restart or
Engine/Penelope evidence; host planning, coverage backfill and unchanged-failure
no-op handling remain unimplemented. Full CI for this extension remains open.

Follow-up: SDK `from_facts` now reconstructs recognized coverage and checks its
canonical node/provenance pair against the observed file; unknown kinds,
missing/malformed diagnostics and mismatched provenance reject. All 15 SDK
tests and strict SDK Clippy pass. RecordFailures preparation validates existing
coverage, backfills unchanged legacy successful files, and retries unchanged
syntax failures while emitting an empty delta when their canonical facts are
unchanged. All 18 Indexer tests and strict Indexer Clippy pass, including legacy
backfill and repeated-failure empty-delta assertions. Engine source planning,
durable publication/restart verification and host reporting remain open; direct
Indexer callers still control whether an empty delta is published. Full CI for
this follow-up has not yet run.

Engine's explicit `with_source_syntax_policy` delegates preparation to Indexer;
publication remains the existing Penelope/StateChronicle path. This is embedded
API opt-in only. CLI/daemon source planning and reporting are not enabled, and
callers must not publish unchanged empty deltas merely to record an attempt.


SQLite's first fixture run correctly rejected opening a nonexistent database:
the wrapper omitted the required explicit migration step. Adding the same
`migrate`-before-`open` bootstrap used by existing conformance fixtures fixes the
fixture without changing store behavior. Both SQLite/Turso shared scenarios and
focused test-target strict Clippy now pass; all three durable backends have
passed this lifecycle scenario. This does not imply every coverage gate is met.

The shared lifecycle now starts with a resolved sibling call into the file that
later fails syntax. File/SQLite/Turso checks verify the old target and incoming
resolved edges disappear while source reference evidence may remain unresolved.
The first strengthened assertion incorrectly rejected any node named `original`,
including legitimate retained reference occurrences; checking the target ID
corrects the test without changing graph semantics. A same-identity producer
fixture returning `InvalidInput` proves partial policy still rejects internal
failure, with graph/history unchanged after reopen. All three scenarios and
strict all-target Engine/Turso Clippy pass without exceptions. These are
pre-publication extraction-failure checks, not mid-publication crash injection.

Full contributor CI for the accumulated SDK/Indexer/Engine coverage work passes
(exit status 0) in 274.56 seconds. This includes all 17 backend conformance
scenarios (45.55 seconds), the shared File/SQLite/Turso processing lifecycle,
strict workspace checks, independent extension conformance and API docs.
Producer-change failed-file retry evidence and host-level availability remain
open; passing CI does not complete the v0–v5 capability roadmap.

Follow-up producer-upgrade preparation now passes in the same File/SQLite/Turso
fixture: unchanged invalid bytes produce the same processing node ID with new
attempted-producer provenance and still decode as syntax failure. Discarding the
prepared upgrade preserves accepted history; an original-producer retry remains
an empty delta. All three lifecycle scenarios and strict all-target Engine/Turso
Clippy pass. This tests upgrade preparation, not accepting and restarting an
upgraded failure generation; full CI after this test addition remains open.

Embedded reporting adds `SourceProcessingCoverage`: selected generation plus
explicit completed, syntax-failed and unclassified file-ID lists. Absence is
unclassified, never successful processing. The initial read materializes the
selected historical snapshot and validates recognized processing fact pairs;
it is not a bounded or O(1) query. Engine rejects generation scope mismatch.
CLI/daemon wire DTOs and source planning remain unchanged until separately wired.

Embedded `source_processing_coverage_at` now passes the shared File/SQLite/Turso
lifecycle: legacy strict-only history is unclassified rather than clean; partial
history has one completed and one syntax-failed file; repaired history has two
completed files. Foreign engine scope rejects before snapshot materialization.
Strict all-target Engine/Turso Clippy passes. The report reads the generation's
manifest separately from its fact-only snapshot, matching existing store
contracts. Full CI for this reporting addition remains open.

The reporting projection's negative regression rejects missing owner/provenance,
missing failure payload, source-hash mismatch and duplicate processing facts.
Absent processing nodes instead remain unclassified even with extraction
provenance present. The focused test passes; no production behavior was changed
by these regression cases.

Read-only CLI `processing-coverage[-turso] <store> <current|generation-id>` exports
one JSON object with schema version 1, generation, completed/unclassified file-ID
arrays and syntax-failed entries containing file ID, message and truncation flag.
It uses the existing Engine reporting API and full snapshot read. Turso requires
an absent-owner writer lease, like `cycles-turso`; active-daemon attachment is not
implemented and must not fall back to a second connection. This command does not
change ingestion policy, publish facts or declare unclassified files clean.

CLI process evidence passes on File/Turso for current partial coverage and
explicit historical legacy/unclassified selection. All-target CLI compilation
and strict all-target/all-feature CLI Clippy pass. Usage and README document
the full-snapshot cost, strict ingestion default and absent-owner Turso limit.
Active-owner rejection and malformed argument process coverage remain to be
added; full CI for this CLI addition has not yet run.

The CLI fixture now also holds the existing writer lease and verifies active-owner
Turso rejection with no result on stdout. Both File/Turso malformed/missing/extra
argument cases fail without creating the requested database file. Both process
tests and strict all-target/all-feature CLI Clippy pass. This covers absent-owner
embedded safety, not daemon attachment; full contributor CI remains pending.

Host integration investigation: `syntaxmesh-source-host/src/reconciliation.rs`
owns the shared immutable-inventory, resolver-input and publication path. Its
`!published` source-planning return currently skips preparation entirely.
Therefore adding a CLI option alone would silently prevent unchanged failed-file
retries. Integrate policy-aware planning and explicit failed-coverage retries in
that shared path before exposing an ingestion flag. Preserve strict-mode
fingerprints/no-op behavior, resolver-input validation and the single Engine
publication path; report a retry with unchanged facts as unpublished. This next
integration is not implemented yet.

Full contributor CI for reporting and the read-only CLI command passes (exit
status 0) in 261.26 seconds, including all 17 backend conformance scenarios
(42.34 seconds), malformed coverage projection, current/historical CLI process
tests and shared durable lifecycle tests. Existing allowed dependency warnings
remain; no new lint exceptions or dependency changes were introduced. Policy-aware
host ingestion and the broader v0–v5 requirements remain unfinished.

Policy-aware Engine planning preserves strict fingerprints and derives a separate
`source-syntax-policy-v1` fingerprint for RecordFailures. Shared reconciliation
retries unchanged failed coverage through preparation and the existing resolver
input stability checks, but does not publish an empty retry. A nonempty retry
under identical source/producer policy fails as a deterministic-extractor
invariant violation; callers must change producer identity when semantics change.
No attempt journal or synthetic successful generation is added.

Shared-host policy-aware planning and failed-file retry are now implemented.
The counting-producer regression verifies strict-to-partial policy transition
publishes coverage under a distinct generation, an invalid edit publishes failed
coverage, and unchanged retry actually calls extraction without accepting a new
generation. StateChronicle history stays verified. The focused regression and
strict all-target/all-feature SourceHost/Engine/Indexer Clippy pass. CLI/daemon
ingestion flags/configuration remain unwired; full CI for this change is open.

CLI indexing explicitly opts in with `--record-syntax-failures`. Duplicate flags
reject. Partial runs label the input count as observed, print completed/failed/
unclassified counts, and describe unchanged inventory without claiming complete
analysis. The existing shared reconciler owns retry/no-op behavior. Project and
daemon defaults stay strict; no per-host alternate publication implementation is
added. Semantic enrichment remains separately opt-in.
