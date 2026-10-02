# ADR-0139: Reviewable semantic evaluation evidence

- Status: accepted
- Date: 2026-09-30

Extend ADR-0138's opt-in Rust evaluator using Shardline's retained incident
evidence pattern rather than introducing an evaluation service or SDK. Export
accepted subject/relation/object triples and their supporting quotations into
the existing JSON report. Resolve them from the actual accepted graph, not the
provider response or substring label checks.

Verify each support edge's semantic provenance, document-chunk endpoint, file
identity/content hash, and exclusive byte range against the retained fixture
source. Reconstruct the relation from the producer's claim label and its exact
subject/object role endpoints. Reject inconsistent review evidence instead of
silently omitting it. Include typed evidence class, producer version, claim and
concept identities, and supporting source paths/ranges. JSON is an evaluation
artifact, not canonical storage or an internal handoff.

CI runs both empty-output and grounded-positive loopback mocks. The positive
case checks all four controlled triples, exact source quotations, and shared
concept identity across two documents, in addition to unchanged-input cache reuse.
Changing retained source bytes must fail the review's accepted-hash check.
These mocks prove evaluator/graph plumbing, not actual model quality, semantic
entailment, cross-document synthesis, or whole-project relevance. Live provider
execution remains explicit and opt-in. No public contract or production graph
format changes.

Verification: `cargo test -p syntaxmesh-cli --test semantic_quality --locked`
passes both automatic mock cases; live inference remains ignored. Focused
Clippy and `cargo make ci` pass. The positive retained report contains four
triples with exact source quotes; unchanged reindexing makes no inference calls
and preserves the accepted generation. No live model was evaluated.
