# ADR-0347: Retained inference premise-closure lifecycle

Status: direct effect, retained reads and opt-in Penelope lifecycle recovery
implemented; new-claim discovery and host delivery pending.

The history and truth-maintenance requirements need exact invalidation time and
reasons, not only an unordered set of affected IDs. Add a bounded Query closure
feed retaining the accepted generation/sequence at which each exact canonical
premise version closed. Reuse the validated generation/fact-version feed logic
from ADR-0344; keep existing invalidation APIs and their foreign-index guards.

The separate inference SQLite adapter adds an explicit v1→v2 named SQL migration,
with transactional checksum-ledger advancement. Ordinary open rejects old schemas
until explicit migration. Backfill lifecycle coverage only to each stored claim
selection's origin, never to current canonical history.

A direct lifecycle effect verifies its origin receipt and canonical scope, seeks
stored premise postings for closed versions, and atomically persists earliest
claim invalidation, exact premise-closure reasons and the checked-through
generation. Positive generation/version/posting/claim/evidence bounds are required;
any failure leaves prior lifecycle coverage intact. Repeated ranges are idempotent;
coverage cannot regress, and restored identities do not revive old versions.
Do not load every claim or replay rules just to invalidate dependencies.

This is direct canonical-premise truth maintenance, not complete incremental
synchronization: discovery/evaluation of newly enabled claims remains separate.
The opt-in Penelope coordinator is available as a library; full-engine user
commands remain separate work. Recursive claim dependencies, contradictions,
ontology migrations and full claim-status vocabulary remain later work.

## Interrupted effects after later coverage advances

Penelope lifecycle orchestration requires a durable receipt for each
completed target range in schema v3. Otherwise an effect committed before its
journal completion becomes unrecoverable once another request advances coverage:
the old request would be rejected as reversed. Retain these receipts in the same
transaction as invalidations and coverage. A retry of a recorded historical
target returns its exact receipt without rewinding coverage or replaying feeds;
an unrecorded reversed target still rejects. Explicit v2→v3 migration backfills
only the last genuinely recorded coverage receipt, not invented historical
effects. Completed receipt lookup verifies the origin and checked coverage.

Lifecycle orchestration reuses the rebuild coordinator's bounded Penelope
journal implementation through private generic request/receipt types, preserving
the existing rebuild record bytes and saga identities. Lifecycle requests use a
separate versioned prefix and definition; identity binds origin operation, target
generation and absolute destination, while the full request digest binds execution
controls. Completion validation reads the exact retained range receipt, not just
the latest coverage watermark. No second workflow engine or scheduler is added.

## Implemented boundaries and verification

`Query::inference_premise_closures` exposes exact closure coordinates without
claim loading. The shared InMemory/File/SQLite/Turso fixture compares those
coordinates with canonical version feeds, and the malformed-feed fixture checks
the public closure API against the existing fourteen corruption cases.

`InferenceMaterializer::invalidate` incrementally advances each origin operation's
checked-through coverage. Indexed posting seeks load and digest-check only matched
claims; unique claim bytes, postings and closure reasons have independent bounds.
Earliest invalidation and all encountered closure reasons persist atomically
with coverage. Recorded-target retries return the exact stored effect receipt;
unrecorded reversed targets reject. `claim_lifecycle` reads the known interval, explicit coverage
and bounded exact reasons without canonical replay. `claims` continues to return
the immutable original selection: neither API invents new claims or asserts
validity beyond checked coverage. No automatic host scheduling is added.

Seven storage tests pass, including v1/v2 migration backfill, interrupted upgrade
rollback, lifecycle publication rollback, retained reopen, restoration without
revival, two premises closing at different generations, earliest-time/evidence
oracle comparison and generation/version/posting/claim/evidence/byte exhaustion
with no partial publication. An EXPLAIN fixture confirms the actual posting query
seeks `inference_premise_postings`. Four Query tests and two SQL history tests
pass, covering all four canonical backends. All twenty-seven Penelope integration
tests pass, including File/Turso lifecycle recovery, interrupted effects recovered
after later coverage advances, immutable controls and missing/corrupt effects.
A compatibility fixture verifies unchanged original rebuild journal bytes.
Strict feature-enabled Penelope/storage Clippy, architecture and the contributor
`inference-migrations` registry gate pass against all three migrations. Full
`cargo make ci` subsequently passed in 484.79 seconds, including all-feature
workspace tests, strict Clippy, dependency gates and API documentation. Live
provider quality evaluation and diagnostic timing probes remain opt-in/ignored.
The preceding run exhausted disk while linking; generated daemon/CLI/MCP build
artifacts were cleaned to make room, preserving source and stored project data.
