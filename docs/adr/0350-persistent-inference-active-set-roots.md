# ADR-0350: Persistent inference active-set roots

Status: persistent roots, root-backed enumeration and explicit v4→v5 backfill
implemented, including bounded point membership, payload and paged reads;
Penelope initialization/synchronization recovery implemented in ADR-0351;
host delivery pending.

## Decision

ADR-0349 interval reads establish correct retained query semantics without rule
replay, but a birth/closure predicate can still examine many historical intervals.
Do not treat that index as completion of history-depth-independent lookup.

Associate each accepted stream checkpoint with an immutable active-claim root.
Use a content-addressed compressed binary trie keyed by the existing 256-bit
claim identity, with immutable leaf and branch pages. Successor synchronization
path-copies only changed claim paths, removes invalidated active IDs and inserts
new IDs in the same transaction as its checkpoint. Unchanged checkpoints reuse
the prior root. Claim payloads and ordered premises remain in their existing
retained tables; roots do not duplicate or reidentify unchanged witnesses.

Key depth is bounded by 256 bits, independent of retained generation count.
Point reads seek the checkpoint/root and traverse bounded immutable pages;
enumeration visits only the selected active tree plus requested payload/posting
output, not previously closed lifetimes. This is fixed-key-depth work, not a
promise of zero disk/index seek cost or O(1) unbounded result enumeration.
Compressed branching avoids allocating 256 physical pages for ordinary updates.

Page encoding and identity are versioned. Reject digest substitution, missing
pages, invalid pivots, inconsistent prefixes/children, cycles and duplicate leaf
keys. Read and write page admission must fail the entire effect/query on
exhaustion. Pages are never overwritten and old roots remain readable after
later synchronization or reopen. Compaction/garbage collection requires a
separate explicit retained-root policy; no implicit history deletion.

Page records include subtree cardinality, verified against child counts and
checkpoint active counts. `begin_stream` and `stream_claims` gain an explicit
positive `page_budget`; `StreamSyncRequest` gains the same control. Charge page
loads and insert attempts together, including immutable-page collision checks.
Keep these unpublished stream APIs' receipt/output semantics otherwise unchanged.

An explicit migration must build roots for genuinely accepted existing stream
checkpoints from retained interval transitions, verify each active count and
publish roots and ledger atomically. It must not reconstruct unknown canonical
history, re-run inference, silently migrate on ordinary open, or invent missing
checkpoints. Root maintenance belongs to the downstream adapter, leaving pure
facts/DTO/protocol and canonical StateChronicle verification unchanged.

## Required evidence

Add `StreamPageRequest` with origin, exact generation, optional `StreamCursor`,
positive claim/byte/premise/page budgets, and `stream_claims_page` returning
`StreamPage { checkpoint, claims, next }`. Cursors bind stream, full selection,
immutable root and last returned active key. Reject substituted bindings and
inactive cursor keys. Ordering is ascending claim identity; seek strictly after
the cursor using compressed branch prefixes, never enumerate previous results.
One extra leaf establishes whether another page exists; charge its tree reads
but do not fetch its payload. Selected payloads use the shared witness validator.
All metadata and output are transaction-pinned. Later heads must not change an
older cursor's results. Exhaustion fails the query, never returns a partial page.

Add `stream_claim(origin, generation, identity, byte_budget, premise_budget,
page_budget)` returning `StreamClaim { checkpoint, claim: Option<Claim> }`.
Positive independent budgets apply even to absent keys. Check root membership
before seeking the exact payload; inactive keys return `None`. In one deferred
transaction, use the enumeration reader's same birth/closure, digest, scope and
ordered-posting validation for present keys. Preserve witness birth generation.
Unknown selections and corrupt selected data fail rather than return absence.

Expose `stream_claim_active(origin, generation, claim, page_budget)` as a
generation-pinned membership query. Return the selected checkpoint and a boolean,
not a payload or proof of witness validity. Reject unknown selections and root
cardinality mismatches; absence is not an error. Traverse only the selected
compressed path, validating visited branch children under a positive page budget.
Pin checkpoint, root and pages in one read transaction. Payload retrieval and
paging remain separate contracts, with their own output validation.

Compare every root against the existing independent interval/witness oracle over
create/close/restore and mixed replacements. Prove historical-root stability,
unchanged-root reuse, bounded path-copy updates, sparse reads that avoid closed
lifetimes, explicit migration rollback, corrupted-page rejection and complete
read/write budget rejection. Integrate root publication into the same atomic
successor effect and Penelope recovery; retain ADR-0349 public selection semantics.

## Implemented projection and verification

The downstream adapter now stores versioned, content-addressed compressed trie
pages and one root per accepted stream checkpoint. Baseline initialization and
successor effects publish pages/roots atomically with their claim state. Empty
successors reuse their prior root. Public `stream_claims` enumerates the selected
root, then seeks exact stream/claim payloads and ordered postings; it no longer
uses an interval predicate to discover active IDs. Receipts and output remain
pinned in one deferred read transaction, and immutable witness birth generations
remain distinct from the queried selection.

Twenty storage tests pass with strict Clippy. Set-oracle fixtures verify canonical
insertion-order-independent roots, compressed bounded path copying and stability
of every old root through insertion/removal. Corrupt digests, malformed pivots,
missing children, invalid cardinality and page/output exhaustion reject. A mixed
replacement rollback fixture verifies that failed checkpoints leave neither new
active pages nor roots. Existing retained read/lifecycle fixtures still pass.

The sparse-history fixture retains 64 closed witness lifetimes and one active
witness. A single tree-page budget reads the active result before and after
reopen, even when closed payloads are deliberately corrupted. The affected
historical root still rejects its corrupted selected payload. EXPLAIN verifies
the payload reader seeks exact stream/claim identity. This is structural evidence
of avoiding closed-lifetime scans, not a corpus-scale latency benchmark or an
O(1) guarantee for arbitrary result sizes and database index seeks.

Explicit migration reconstructs roots from accepted retained birth/closure
transitions, checking ordered checkpoint coordinates, counters and the stored
head without canonical replay. Original and migrated roots match. Bad active
counts and interrupted ledger publication roll back the entire schema/backfill;
ordinary open rejects the prior schema. The five-entry SQL registry gate now
recognizes explicit Rust backfill hooks. Latest verification is targeted; the
previous completed full CI predates this projection.

`stream_claim_active` now returns generation-pinned checkpoint/membership without
enumerating the selected set or reading claim payloads. Visited children, pivot
ordering and root cardinality are validated within the page budget. Set-oracle
checks cover all candidate keys during 64 insertions; independent page/count
exhaustion and corrupt-root checks reject. The sparse-history fixture verifies
current and old membership with one page, rejects unknown generations/zero
admission, and reopens without canonical storage even with corrupted closed
payloads. Twenty storage tests and strict all-target Clippy pass. Membership is
not payload/witness verification; that validation belongs to the payload readers.

Exact `stream_claim` payload reads now share the enumeration reader's validation
helper. A present selected key verifies birth/closure coordinates, digest/scope
and all ordered postings in the same pinned transaction. Sparse-history tests
compare point results with enumeration, reject insufficient/zero byte, posting
and page bounds, distinguish inactive keys from corrupt historical payloads,
and reopen without canonical storage. A one-leaf selection uses one tree page.
These point reads are implemented; no new full-workspace CI is claimed.

Paged reads now seek strictly after a cursor bound to stream, selection and root,
validate cursor membership and root cardinality, and fetch only selected exact
payloads using the shared witness validator. A bounded extra tree leaf determines
continuation without fetching its payload. Cursor validation and output share a
page budget and deferred transaction. Output exhaustion rejects instead of
returning a partial page. Cursors are selection-bound continuation coordinates,
not authenticated capabilities or authorization tokens.

Twenty-two storage tests and strict all-target Clippy pass. Sixteen-witness
fixtures compare concatenated pages with complete output at page sizes 1, 3, 4,
16 and 17, enforce exact byte/posting boundaries, and reject exhausted limits,
foreign roots/streams/selections and inactive cursor keys. Later retraction and
reopen preserve the old cursor; empty roots terminate explicitly. Corrupt prior
and lookahead payloads do not affect the selected page, but corrupt selected
payloads reject. Sorted-set differential tests exercise every bound over 64
historical roots. A 1024-key tree seeks its last result with exactly 21 page loads
and rejects a budget of 20; corruption deep in an earlier subtree does not affect
that seek. This verifies tree pruning, not a whole-tree integrity audit, a
database latency benchmark or zero-cost cursor verification. Durable recovery
is recorded in ADR-0351; host-integrated delivery remains open.

The sparse-history paged reader also returns its one selected witness within one
tree-page budget after 64 closed lifetimes and reopen. Architecture and the
five-entry migration gate pass; all 27 existing feature-enabled Penelope
integration tests pass against the current adapter. Those regression tests cover
existing rebuild/lifecycle journals. ADR-0351 records subsequent initialization
and synchronization recovery tests; the earlier 27-test run did not cover them.

Full contributor CI now covers this projection and ADR-0351's durable recovery,
passing all-feature workspace tests, strict Clippy and API documentation in
302.95 seconds. Debug symbols/incremental compilation were disabled for disk
admission, not assertions or gates. Allowed dependency warnings and ignored
opt-in live-model tests remain. This does not establish corpus-scale latency,
live semantic quality or completion of host-integrated delivery.
