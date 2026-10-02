# ADR-0004: Opt-in generation verification

- Status: accepted
- Date: 2026-09-27

## Context

The foundation rules require StateChronicle generation-verification support
from the first implementation phase, with runtime verification opt-in. A
normal index call must not change a durable generation to `VERIFIED` without
its host explicitly requesting that policy.

## Decision

Keep the StateChronicle adapter in the first-phase dependency graph and expose
verification as an explicit engine/host option. Engine construction defaults
to verification disabled. An unverified successful publication remains
`DURABLE`; an enabled verifier transitions through pending to verified or
failed status. The CLI offers an explicit `--verify` option for both storage
backends. The local adapter appends and replay-checks a durable hash-linked
manifest history through the store's opaque durable-record port; the exact
contract is in [ADR-0023](0023-durable-statechronicle-generation-history.md).

## Consequences

- The default indexing path does not imply verification that a host did not
  request.
- Verification-enabled and default paths publish identical graph facts and
  generation identities; only workflow status and the verification receipt
  differ.
- StateChronicle remains compiled and test-covered from the first
  implementation phase. The local history chain does not claim signatures,
  external timestamps, or protection from an actor rewriting the whole store.
- This decision does not make Penelope publication durable by itself; durable
  process/event persistence and restart recovery remain a separate acceptance
  gate.

This ADR supersedes the unconditional verification behavior described in
[ADR-0002](0002-engine-workflow-integrations.md); the Penelope publication
composition decision in ADR-0002 remains in effect.
