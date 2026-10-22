# ADR-0351: Penelope inference stream synchronization

Status: opt-in durable initialization and synchronization implemented;
host-integrated delivery pending.

## Decision

Add opt-in `PenelopeInferenceStream<S>` to the existing downstream Penelope
integration. It owns immediate-successor synchronization, not canonical mutation,
and takes an explicitly migrated inference database and accepted complete baseline.
Initialization is a separately journaled step before successor synchronization.

Journal an owned immutable bounded request before the SQLite effect, using the
existing versioned Penelope linear-saga journal. Bind canonical target sequence,
origin, rule configuration, destination and every control. Semantic keys exclude
execution controls; changed controls must still reject through request digest.
Keep existing rebuild/lifecycle journal layouts and prefixes unchanged.

New keys sort by fixed-width canonical target sequence before semantic digest.
Recovery validates paged key ordering and each request/key/digest before execution,
so pending successors are recovered in sequence rather than hash order. Never
skip intermediate accepted generations or rewind a later head. Failed effects
stay prepared; prepared requests with an already published checkpoint reuse the
retained effect. Completed retries first require an exact retained checkpoint,
then validate the bounded cached effect; a missing effect must not be recreated
under a completed journal. No implicit migrations, workflow scheduler, transport
or runtime dependencies enter pure crates.

## Required evidence

Extend the coordinator with `begin(database, &StreamBeginRequest)` where the
request binds a previously accepted complete baseline and independent positive
claim/byte/premise/page bounds. Use a separate versioned initialization prefix
and owned request, excluding controls from semantic keys but not request digests.
Journal before `begin_stream`, and recover initialization before successors.
Completed initialization retries must require the existing origin checkpoint
before validating the idempotent baseline effect; never recreate a missing
completed effect or reset an advanced head. Migrations and complete baseline
rebuild/recovery remain explicit prerequisites. Reuse the Penelope journal
execution path without changing existing serialized record layouts.

File and Turso journal reopen before SQL publication and after SQL publication
but before journal completion; restoration after retraction; old retries after
later heads; immutable controls and rule substitution; corrupted/missing journal
or effect rejection; ordered multi-successor recovery. Existing journal byte
compatibility and adapter tests must remain green. Host delivery and automatic
complete-baseline orchestration are not completed by this coordinator.

## Implemented evidence

`PenelopeInferenceStream` journals immutable successor requests before effects,
validates canonical sequence coordinates, recovers ordered prepared records and
requires retained checkpoints on completed retries. The new sequence-prefixed
keys leave existing rebuild/lifecycle layouts unchanged.

All 29 feature-enabled Penelope integration tests pass. File and Turso fixtures
prepare two successors in reverse order with the effect database unavailable,
then reopen and recover retraction before restoration. Resetting only their
generated journal completions simulates SQL publication before completion; old
and current receipts are recovered without rewinding active roots. Tests reject
changed controls, configuration/sequence substitution, corrupt journal bytes and
missing completed effects. Existing journal byte-compatibility tests still pass.
This is targeted validation, not a completed new full contributor CI run.

Durable `begin` now uses a separate initialization journal and immutable
`StreamBeginRequest`. Combined recovery processes initialization before ordered
successors. Completed retries require retained initialization before the bounded
baseline effect is validated; advancing the head never resets the baseline root.
Explicit migration and an accepted complete baseline remain prerequisites.

All 31 feature-enabled integration tests and strict all-target Clippy pass.
New File/Turso fixtures recover a prepared initialization followed by a prepared
successor after reopening; they then simulate initialization SQL publication
without completion after that successor advanced the head. Retained active roots
and old receipts stay unchanged. Changed controls, zero admission, corrupted
initialization journals and missing completed streams reject; a missing stream
is not silently recreated even when its complete baseline is still available.
The 22 storage tests and architecture gate also pass. Host command orchestration
remains outstanding.

Full contributor CI for this initialization/synchronization snapshot passed in
302.95 seconds, including all-feature workspace tests, strict Clippy, migration
and dependency policy checks, and API documentation. To fit available disk,
dev/test debug symbols and incremental compilation were disabled; assertions and
validation gates were unchanged. Policy-allowed dependency warnings and ignored
opt-in live-model tests remain. This is implementation integration evidence, not
full v0–v5 completion or host delivery.
