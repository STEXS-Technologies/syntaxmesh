# ADR-0392: One-shot CLI host for external extensions

Status: accepted

Date: 2026-10-23

## Context

ADR-0391 added bounded process supervision as a host adapter, but only the
conformance task composed it with the Engine. A producer could not yet be run
by a user-facing SyntaxMesh host. The existing file-frame import already
defines grant validation, Engine publication, Penelope durability, optional
StateChronicle verification, and CLI ownership behavior.

## Decision

Add `run-extension` and `run-extension-turso` as explicit one-shot host
commands. They accept an initialized store, operator-owned manifest grant,
caller-supplied run/generation identities, a bounded timeout, and an explicit
program/argument vector after `--`. Environment inheritance is cleared by
default; the operator may allowlist named variables. The command acquires the
same writer lease as other publication commands before launching the producer,
validates exactly one authorized IPC frame through `ProcessSupervisor`, then
publishes the returned batch through the existing Engine operation. Optional
StateChronicle verification uses the existing opt-in publication path.

The producer is not implicitly configured or launched by indexing/watch/daemon
startup. Its executable and arguments are explicit operator policy. The CLI
does not claim to sandbox the producer: process-group cleanup is not an OS
security boundary. Daemon configuration/scheduling, authenticated remote peers,
and stronger OS containment remain separate work.

## Consequences

- The supervised adapter now has a user-facing Engine/Penelope publication
  composition path without adding process ownership to Engine or core crates.
- Malformed, unauthorized, timed-out, failed, or oversized producers fail
  before publication; the writer lease remains held across launch and commit.
- Operators must initialize the target store first and supply stable unique
  workflow identities for each run.
- A producer receives no ambient environment variables unless explicitly
  allowlisted; its current working directory remains the caller's working
  directory.
- This does not complete daemon-managed extension hosting or deployment
  sandboxing.

## Verification

The external producer conformance fixture exercises both the process adapter
directly and the CLI host against File and Turso stores, including authorized
publication, verified-history opt-in, malformed process output, timeout, and
unchanged graph state after failure. The thin producer remains independent of
Engine, stores, and workflow integrations.
