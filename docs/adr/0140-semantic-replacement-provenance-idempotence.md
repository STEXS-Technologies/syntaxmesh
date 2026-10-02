# ADR-0140: Semantic replacement ignores unrelated retained provenance

- Status: accepted
- Date: 2026-09-30

A CLI regression reproduces an unchanged cached semantic run publishing another
generation after a model revision change. Canonical deltas intentionally do not
remove provenance: earlier records remain retained even after their nodes and
edges are replaced. Comparing all namespace provenance to only the desired
batch therefore incorrectly treats retained, inactive records as a graph change.

Reuse the existing graph-store retained provenance and generation-history
model. Compare the exact current semantic node/edge sets as before, and compare
only retained provenance IDs requested by the replacement batch against its
provenance payloads. Existing store publication validation still requires all
fact provenance references to resolve. Missing or changed desired provenance still forces
publication. Unrelated retained provenance does not. Do not prune provenance,
change deltas, add tables/migrations, or bypass Penelope/StateChronicle.

The CLI integration fixture must repeat after a model revision change with zero
inference requests, cache reuse, and unchanged generation/history length. A
runtime-neutral engine fixture must exercise replacement, clearing, and repeated
clearing, retaining prior graph/provenance history. This corrects documented
idempotence; it introduces no new public contract.

Verification: the extended CLI fixture failed before the fix, reporting
`provider_requests=0` but `published_generation` after the revision change.
After the fix it reports `already current` and preserves generation/history
length. The embedded replacement/clearing fixture and full `cargo make ci`
pass. This is verified publication idempotence, not live-model quality evidence.

Canonical-host follow-up: the same CLI scenario now runs against explicitly
migrated Turso with `--verify`, reusing the shared fixture rather than adding a
second implementation. Every reopen checks `Verified` status; unchanged cached
runs and rejected late model changes preserve generation/history length. The
final `statechronicle-verify-turso` command verifies the complete retained chain.
File/reference and Turso/verified cases both pass, as does focused Clippy. The
provider is still a loopback mock, not evidence of model extraction quality.
