# ADR-0365: Registered Inference Stream Setup

Status: accepted

Date: 2026-10-22

## Context

Registered inference can rebuild a complete baseline and synchronize an
immediate successor, but the engine setup API only creates an unregistered
baseline stream. Registered callers therefore lack one durable operation that
binds the ontology-aware baseline to the registered stream configuration.

## Decision

1. Add registered stream initialization as a separate Penelope parent workflow
   and runtime-neutral Engine operation. The parent intent is persisted before
   either child effect and retains ontology definitions, admission bounds,
   rebuild controls, and stream page budget for recovery.
2. Initialize the stream using the exact bound rules and registered baseline
   receipt. Keep existing unregistered setup requests and journal keys intact.
3. Recover registered setup parent intents as part of inference recovery before
   registered stream successors, using a new versioned journal prefix.
4. Require explicit downstream migrations and enforce engine repository and
   worktree scope before preparing the parent journal.

## Consequences

Registered rebuild, stream initialization, and successor synchronization now
have a composable runtime-neutral path. No existing storage schema or journal
payload is rewritten. Recovery retains the full caller configuration rather
than requiring the original setup request.

## Verification

File and Turso restart fixtures interrupt after parent-intent persistence but
before the downstream SQLite effect, reopen the canonical store, recover using
the retained request, then publish a registered successor. The Engine fixture
also verifies registered baseline setup followed by registered synchronization.
InMemory, File, and Turso fixtures cover this recovery path. All twenty Engine
tests and thirty-eight feature-enabled Penelope tests pass; strict all-target
Clippy, formatting, and the architecture dependency gate pass. Full contributor
CI has not yet been rerun.

## References

- [ADR-0352](0352-engine-inference-application-service.md)
- [ADR-0353](0353-durable-inference-setup-intent.md)
- [ADR-0363](0363-penelope-registered-inference-rebuilds.md)
- [ADR-0364](0364-registered-inference-stream-successors.md)
