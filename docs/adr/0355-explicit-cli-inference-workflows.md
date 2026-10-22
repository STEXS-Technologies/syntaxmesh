# ADR-0355: Explicit CLI inference workflows

Status: bounded configuration preflight, explicit migration and recovery commands
implemented; setup and synchronization pass targeted lifecycle and strict lint
checks; detached checkpoint and bounded page delivery pass targeted tests.

## Decision

Deliver opt-in inference through CLI commands using the engine application
service, not copied journal or synchronization logic. Support existing File and
Turso canonical stores. Keep the downstream SQLite path explicit and separate
from source trees and canonical storage; migration is a distinct command.

Provide setup, recovery, immediate-successor synchronization, checkpoint and
bounded retained page commands. Setup and synchronization take explicit canonical
generation IDs and a bounded, versioned JSON configuration containing rules and
independent execution/admission limits. Reject oversized files before decoding,
unknown fields, unsupported versions, empty rule sets and invalid limits. Do not
invent default ontology/rule semantics, silently skip accepted generations or
automatically rebuild a changed rule configuration.

Use complete structured origin receipts for subsequent commands, preserving
operation identity and scope rather than reconstructing receipts from a stream
ID. Bound receipt/cursor decoding as well. JSON is host input/output, not canonical
storage or a service-to-service intermediate; no NDJSON journal is introduced.

Recovery executes existing durable intent without needing the original rule
configuration. New effects require engine scope admission. Reads remain explicitly
generation-pinned and do not trigger recovery, migration or canonical replay.
Do not imply that successive cross-database effects form one atomic transaction.

## Required evidence

Binary-level File/Turso tests must cover explicit migration, setup, successor
retraction/restoration, retained pagination, retry and reopen recovery. Reject
invalid configuration, missing/unmigrated destinations, foreign scope, changed
immutable requests and substituted cursors. Verify ordinary reads and failed
admission do not create storage or rewrite canonical generations. Keep production
logic in focused sibling modules and substantial tests in dedicated modules.

HTTP/MCP and pure versioned transport DTO integration remain separate follow-up
work. These commands do not complete higher inference or the v0–v5 roadmap.

## Initial implemented evidence

The opt-in `syntaxmesh-cli/inference-sqlite` feature exposes
`inference-config-check <configuration.json>`. It reads at most 1 MiB plus an
overflow sentinel before decoding, rejects unknown root/rule/limit fields,
duplicate version fields, unsupported versions, empty/duplicate rule identities
and zero admission, and reports the pure rule-set fingerprint without storage
access. Three dedicated parser tests, strict all-target feature-enabled Clippy
and the architecture gate pass. Execution budgets leave semantic identity
unchanged. This preflight is not rule evaluation or end-to-end workflow delivery;
setup, migration, recovery, synchronization and retained read commands remain
unfinished. Full CI recorded for ADR-0354 predates this change.

The next slice implements `inference-migrate <inference.sqlite>` and
`inference-recover[-turso] <canonical-store>`. Migration leases its destination;
recovery leases canonical storage before opening and delegates to engine recovery.
Recovery reports versioned baseline/setup/stream request counts, without requiring
the original rule configuration. Two binary-level tests pass for File/Turso:
explicit idempotent migration, lease exclusion, empty recovery after reopening,
unchanged canonical status, and missing-store/extra-argument rejection without
storage creation. Strict all-target feature-enabled Clippy passes. Prepared
inference jobs remain covered at engine/coordinator level; binary-level prepared
job recovery and complete workflow delivery still need verification.

The setup command is now wired as `inference-setup[-turso] <canonical-store>
<inference.sqlite> <generation-id> <configuration.json>`. It admits arguments,
generation and bounded configuration before storage access, requires existing
stores, leases both destinations, validates the downstream migration ledger and
delegates to the engine parent workflow. It returns the complete origin receipt.
Expanded File/Turso binary fixtures cover explicit migration, retained checkpoint
creation, exact retry and downstream lease exclusion. Verification of this latest
slice is pending: the initial fresh dependency build suffered linker bus errors
before tests ran; a reduced-parallelism retry is in progress. Formatting and
`git diff --check` pass. Do not treat these fixtures as passing evidence yet.

Immediate-successor synchronization is now wired as `inference-sync[-turso]
<canonical-store> <inference.sqlite> <generation-id> <configuration.json>
<origin.json>`, using the engine coordinator with both destination leases and
explicit migration validation. Origin receipt input is bounded to 16 KiB before
decoding; the engine/materializer validates scope and retained identity. Binary
fixtures now request a real indexed successor, retry its receipt and retry setup
after the advanced head. This code is also awaiting verification while the fresh
dependency build runs. No synchronization success is claimed yet.

### Subsequent targeted verification

After the fresh dependency build, two binary tests pass on 2026-10-21. File/Turso
fixtures now create real claims from indexed Rust call edges, retract the terminal
declaration, synchronize an immediate successor, restore the declaration and
verify new claim identities rather than reviving old premise versions. Baseline
claims remain readable after retraction. Exact setup/sync retries preserve origin
and checkpoint receipts, including setup after an advanced head. Explicit
migration and downstream lease exclusion pass. The fixture's output relation is
external as required by the pure inference contract; native output relations are
not supported. Five configuration/receipt admission unit tests pass. Formatting
and diff checks pass. Strict all-target Clippy is running; full contributor CI has
not been rerun for this CLI slice. Retained read commands and binary-level
prepared-job recovery verification remain unfinished.

Strict all-target feature-enabled Clippy subsequently passed for this CLI slice
on 2026-10-21. Retained read commands will use a detached engine scoped by the
supplied complete origin, with an empty canonical reference store. The existing
materializer validates that exact origin against retained downstream receipts;
the origin is a local selection, not an authentication credential. No canonical
store is opened and no history is loaded or replayed for these commands. This
reuses the already-tested engine read API without duplicating root, cursor or
payload validation in the host. Bounded origin/cursor admission precedes opening;
reads neither migrate nor recover workflows. Implementation remains pending.

The detached checkpoint command is now wired as `inference-checkpoint
<inference.sqlite> <generation-id> <origin.json>`. Its expanded File/Turso fixture
requests the retained baseline while holding canonical writer ownership and
rejects a missing downstream path without creating it. This latest read slice is
now covered by two passing binary integration tests, including the detached
checkpoint cases, on 2026-10-21. The architecture executable's default linker
crashed with a bus error; relinking that executable with BFD is in progress, not
a passed architecture gate. Bounded page delivery remains unfinished.

The bounded page command is now wired as `inference-claims-page <inference.sqlite>
<generation-id> <origin.json> <claims> <bytes> <premises> <tree-pages>
[cursor.json]`. It admits positive independent limits before opening downstream
storage, bounds cursor input to the same 16 KiB receipt limit, and delegates to
the detached engine's retained page API. File/Turso fixtures now contain multiple
claims and drain one-claim baseline pages while canonical ownership is held,
reject substituted roots without output, and query empty retracted selections.
This latest page slice is awaiting tests and strict lint checks; tests are queued
behind the architecture executable's BFD relink. No new page verification is
claimed yet.

### Subsequent retained-page verification

Two binary integration tests pass with the multiple-claim File/Turso corpus on
2026-10-21. One-claim pages drain to exactly the retained baseline claim set;
substituted-root cursors fail without output, and a retained retracted checkpoint
returns an empty page. These reads succeed while canonical ownership is held.
Strict all-target feature-enabled Clippy passes. The architecture gate passes
using the unchanged xtask sources, after linking only that executable with BFD
to work around the default linker's bus error; no repository linker policy was
changed. This is not a completed full contributor CI run. CLI prepared-job
recovery, broader malformed-origin/cursor admission and host transport delivery
remain open.

A dedicated binary recovery fixture now prepares setup against a missing
downstream database through the public engine, asserts that parent and baseline
intent survive, drops the engine, explicitly migrates via CLI and invokes
`inference-recover[-turso]` without rule/configuration arguments. It checks exact
first/second recovery counts and retained initialization with the two expected
Rust-derived claims on File/Turso. This additional fixture is awaiting its
targeted test result; it is not yet passing evidence.

The dedicated prepared-parent recovery test subsequently passes on File and
Turso on 2026-10-21, including exact first/second recovery counts and two retained
claims without request resubmission. Strict all-target feature-enabled Clippy,
formatting and diff checks pass. This extends binary-boundary evidence beyond
empty recovery; other interruption boundaries remain covered at coordinator and
engine level rather than exhaustively through the CLI. Full contributor CI for
the accumulated CLI implementation is the next verification gate.
