# ADR-0143: Cache validated recovery leaves despite sibling failure

- Status: accepted
- Date: 2026-09-30

## Context

Graphify checkpoints successful semantic chunks so interrupted extraction can
reuse completed work. SyntaxMesh's Penelope batch workflow already completes
successful document outputs while leaving failed requests prepared. However,
the CLI recovery adapter currently collapses a split group into one error if
either child fails, discarding the other child's validated inference.

## Decision

Return one result per original request from bounded adaptive recovery. Validate
each successful leaf's exact evidence and independent document ownership before
returning it. Preserve deterministic request order and the existing three-level
split budget. Ordinary errors do not split; sibling leaves may still complete.
Reject provider output cardinality mismatches rather than pairing wrong results.

Reuse the existing Penelope durable completion records; add no cache tables,
public DTOs, transport, runtime, or database dependency. A failed overall run
does not publish a partial semantic graph. A subsequent normal index invocation
reuses completed documents and sends only remaining prepared requests.

This supersedes ADR-0142's limitation that successful recovery leaves are lost
when a sibling fails. Failed or invalid claims remain retryable, not cached.
Cross-document synthesis and real-model quality remain separate open gates.

## Verification

Require deterministic recovery tests with successes on either side of a failed
leaf, bounded execution, and rejected cardinality mismatches. CLI fixtures must
fail after one recovered document succeeds, restart with only the failed
document dispatched, then repeat with the provider closed. Run this on File and
verified Turso and verify no partial semantic generation was published.

These fixtures pass: the retry reports one provider request and one cache hit,
the final repeat reports zero provider requests and two cache hits, and the
published coalesced claim retains both documents' support edges. The test server
also checks that the retried content hash belongs to the failed document.

## References

- [ADR-0142](0142-bounded-semantic-truncation-recovery.md)
- [ADR-0116](0116-penelope-semantic-enrichment-cache.md)
- Graphify `graphify/llm.py`, successful-chunk checkpointing.
