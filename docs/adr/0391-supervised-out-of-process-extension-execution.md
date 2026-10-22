# ADR-0391: Supervised out-of-process extension execution

Status: accepted

Date: 2026-10-23

## Context

The framed extension codec and Engine import path already validate and publish
`FactBatch` values, but only a host that owns the producer stream can impose a
deadline, bound output, and clean up a failed producer. File import does not
launch or supervise producers, and the Engine must remain runtime-neutral and
free of process ownership.

## Decision

Add a separate `syntaxmesh-extension-process` host adapter. It accepts a
caller-configured Rust `Command` plus an independently operator-authorized
`ExtensionManifest` grant; executable choice, arguments, environment, and
working directory remain host policy. The adapter disables stdin, captures
stdout/stderr concurrently, enforces a positive deadline and hard output caps,
spawns an isolated process group using `command-group` 5.0.1, and terminates
the group on timeout, stdout overflow, or producer exit. It accepts only a
successful process that emitted exactly one complete, authorized `SMEX` frame
and no trailing bytes. Stderr is drained while only a bounded prefix is
retained. The returned `FactBatch` is passed to the existing Engine ingestion
method; the process adapter does not open stores or own Penelope or
StateChronicle workflows.

The process group is cleanup and failure isolation, not a security sandbox.
Hosts remain responsible for command trust, credentials, filesystem/network
policy, peer identity, scheduling, and any stronger OS containment. The adapter
is a composition primitive; normal indexing and the file-import CLI do not
launch external producers.

## Consequences

- A producer timeout, nonzero exit, oversized output, malformed frame, or
  unauthorized manifest fails before Engine publication.
- Descendants in the supervised process group are terminated and reaped on
  timeout and after the producer leader exits.
- Core, DTO, SDK, and runtime protocol crates gain no process or host
  dependency; the host adapter depends only on the SDK and IPC adapter.
- Daemon configuration, authenticated peer handling, concurrency policy, and
  production OS sandboxing remain separate integration work.

## Verification

The out-of-tree Rust producer conformance gate runs through the supervisor and
checks authorized success, partial-frame rejection, bounded stdout, deadline
termination, and descendant cleanup. It then imports the batch into File and
Turso through the existing Engine path and verifies the opt-in StateChronicle
history. Focused process/xtask tests and `cargo make extension-fixture` pass.
