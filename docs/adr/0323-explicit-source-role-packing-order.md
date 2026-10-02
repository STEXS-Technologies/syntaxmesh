# ADR-0323: Reuse typed role evidence for explicit packing order

Status: explicit ordering and boundary tests verified; full CI passes; corpus gate pending.

Full contributor CI passes in 257.44 seconds, including all 17 backend/restart
scenarios (41.19 seconds), both Turso role-history cases, strict checks and docs.

The first full CI invocation stops at the independent extension fixture's
`--locked` check: its separate lockfile also needs the existing Query-to-SDK
dependency edge. Offline fixture checking updates only that dependency list,
with no package/version changes, and passes. The generation-pinned Query service
adds the same thin delegate as existing neutral lexical plans. Rerun full CI;
the stopped invocation is not a correctness or quality pass.

All 61 Query tests and strict Query Clippy pass, including exact positive-role
accounting (64-byte total rejected; 65 accepted for 33 existing metadata bytes
plus one 32-byte role ID) and malformed reserved-role rejection. The new test
caught double charging through the existing helper, which already adds NodeId
size; using zero term bytes restores the intended 32-byte charge.

All 60 Query tests and strict Query Clippy pass for initial implementation.
The new ordering test checks exact neutral parity, stable first/last partitions,
and full membership retention. SDK dependency is pure; the lockfile changes
only the existing Query dependency list. Explicit role memory-accounting and
malformed-payload regression tests are added next and require verification.
Host routing remains neutral; cross-backend/corpus quality and full CI are open.

Retain the existing neutral lexical packing plan and add explicit neutral,
test-intent-first and test-intent-last preferences. Stable partitioning changes
order only: retain every selected ID, lexical work bounds and generation identity.
Unknown remains unknown, not production evidence. Never infer intent from query
words, filenames or missing metadata. Default host behavior remains neutral.

Reuse the SDK decoder when constructing planner metadata, fail closed on malformed
recognized role payloads, and charge 32 bytes for each retained positive-role ID.
Existing `NodeKind::Test` is also positive intent. Query depends only on the pure
SDK contract, not a language producer. Corpus evidence must precede default
promotion; explicit test queries must be evaluated as well as implementation queries.
