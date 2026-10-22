# ADR-0344: Bounded inference invalidation range query

Status: implemented for direct read-only invalidation.

Add an optional read-only Query operation identifying retained claims affected
between their generation and the selected later generation. Verify scope and
contiguous accepted transitions, consume generation-pinned fact-version change
pages, and collect closed exact premise versions. Match the complete set through
the scoped dependency index with explicit generation/version/posting/output bounds.
Failure returns no apparently complete prefix. Same-generation selection is a
validated empty range; reversed ranges reject.

Reuse existing indexed SQL feeds. Reference backends may scan internally;
generation pages include deltas, so logical page bounds are not byte/heap bounds.
This computes direct invalidation identities, not new claims, durable retraction,
recursive propagation or a Penelope-managed publication workflow.

Verification: the shared four-backend fixture agrees with an exact closed-version
oracle, verifies same-generation empty ranges and spans deletion/restoration
without reviving old witnesses. Generation, version and posting exhaustion reject
instead of truncating. Targeted Query/SQL lifecycle tests and strict all-target
Clippy pass. Broader range corpora are still required before release; no automatic
durable invalidation is delivered.

Exact-budget follow-up: a shared four-backend regression first failed when a
fact-free accepted transition followed deletion/restoration after all two version
entries were consumed. A bounded one-entry probe now accepts the empty transition
and still rejects any additional version without returning partial results.
Targeted Query/SQL tests and strict Clippy pass. The preceding stable workspace
CI passed in 209.44 seconds; this follow-up has targeted verification only.

Malformed-feed follow-up: a feed-only store fixture rejects all graph, history
and write access while delegating only manifests, generation sequences and the
two paginated change feeds. The valid range succeeds. Eleven substituted-page
cases reject broken sequence/parent/scope, premature termination, mismatched fact
identity, invalid intervals, duplicate entries and foreign cursors. Foreign empty
dependency indices and reversed ranges also reject. The targeted fixture and
strict Query Clippy pass. This is port-routing evidence, not proof that delegated
reference-store internals avoid history scans.

Version-coordinate hardening now checks opening and closure generation IDs against
authoritative sequence lookup before using their exact references. A twelfth
substituted-feed case replaces a closed version's opening generation while leaving
its plausible sequence unchanged; lookup rejects. Four-backend lifecycle tests,
the feed-only rejection fixture and strict Query Clippy pass. These additional
indexed coordinate reads do not authenticate arbitrary custom store implementations.

Transition binding also requires the page manifest's parent, canonical generation
sequence and full manifest to agree with independent retained lookups. Two more
feed cases substitute parent and graph root and reject. Four-backend lifecycle
fixtures, all fourteen malformed-feed cases and strict Query Clippy pass. This
validates manifest agreement, not a run-ID binding: manifests carry no run ID.

Operational-status follow-up: SQL manifest lookup reflects later verification
status updates whereas accepted change pages retain publication-time status.
Normalize only status before comparing manifests; all immutable coordinates must
still match. The shared fixture reproduced this SQL/reference difference and now
verifies invalidation after a status update across all four backends. Targeted
lifecycle/malformed-feed tests and strict Query Clippy pass. Status normalization
does not waive graph-root or verification-policy requirements.
