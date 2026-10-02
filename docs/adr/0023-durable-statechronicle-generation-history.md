# ADR-0023: Durable StateChronicle generation history

- Status: accepted
- Date: 2026-09-27

## Context

ADR-0004 makes StateChronicle verification opt-in, but the first adapter only
hashed a manifest in process memory. It neither persisted a generation record
nor checked prior history after restart. That does not satisfy the source
architecture's durable, replay-checkable verified-history foundation.

## Decision

The full-engine `GenerationVerifier` receives the existing opaque
`DurableRecordStore` port. When enabled, the StateChronicle adapter appends a
versioned, repository/worktree-scoped manifest record to a persistent
hash-linked sequence using StateChronicle's digest primitive. Each verification
re-reads and validates the sequence, including each manifest digest, record
digest, scope, ordinal, and parent link, before appending the next record.
Exact retries are idempotent; conflicting or corrupt history fails closed.

The initial record is an explicit anchor, so enabling verification on an
existing graph does not imply that earlier generations were verified. This is
local integrity/replay checking, not a signature, external timestamp, or
protection against an actor rewriting the entire store. Signing and portable
proofs remain later work.

## Consequences

- Verification remains opt-in and graph facts/identities remain unchanged.
- Verification requires a backend implementing durable records (all current
  full-engine backends do).
- StateChronicle history survives process restart and detects missing,
  reordered, malformed, or modified records.
