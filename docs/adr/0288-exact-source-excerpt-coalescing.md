# ADR-0288: Coalesce identical source excerpts without losing fact identities

Status: accepted; contributor verification complete; retrieval quality unproven.

Context packing currently keys source evidence by node identity, repeating the
same rendered excerpt for distinct facts. Reuse the existing multi-node ContextItem
contract and sorted candidate stream rather than introducing another retrieval
service or external reranking API.

After ranking and key deduplication, coalesce SourceEvidence candidates only when
their source path, line boundaries, complete rendered text, evidence class and
edge identities are equal. Retain the first candidate's ranking/key and union
node identities in stable order. Different provenance evidence classes remain
separate. Do not merge overlaps, equal text at different locations, signatures,
paths or summaries. Source validation still precedes coalescing.

Omission counts describe resulting excerpt items, not contributing facts. Every
contributing fact remains attributable through node_ids. The bounded candidate
stream permits a simple equality scan without a new index or dependency.

Verify identity retention, rank preservation and nonmerge boundaries, and extend
the existing temporal backend fixture across current/history/edit/reopen. This
reduces duplicate payloads; it does not establish repository retrieval quality
or accept a family allocation policy.

All 49 Query unit tests and strict Query/Turso all-target Clippy pass. The temporal
backend scenario passes across all four stores, source edits and durable reopen.
Its four facts now share one excerpt with all four IDs, superseding ADR-0287's
four-item assertion; the separate candidate-order fixture retains granularity
precedence coverage. Full contributor CI and repository-quality checks remain open.

Full contributor CI subsequently passes in 166.97 seconds, including all 17
backend scenarios, strict workspace Clippy, boundary/migration checks and docs.
Repository-quality evidence remains a separate gate.

Whole-Sim run `context-20261002T021240.771653Z-uncommitted` completes with the
required-target quality failure, not an ingestion failure. The unchanged 2,572-file
fingerprint publishes in 361.498 seconds; identifier indexing takes 75.317239 seconds.
Default retrieval remains 3/10 and ranked-family source admission remains 3/5 at
8,192 tokens. TypeScript now reaches selection at depth one (ADR-0286) but misses
source packing; Python is selected at depth zero and also misses packing. Exact
excerpt coalescing is visible in multi-node source items but does not solve these
failures. Equal-family allocation remains rejected for defaults.

Completed artifact inspection, without reindexing: Python admits 15 source
excerpts totaling 28,485 rendered UTF-8 bytes, including the 16,613-byte dashboard
excerpt (about 58% of source bytes). Only two admitted excerpts carry multiple
coalesced IDs. TypeScript admits 21 excerpts totaling 20,383 bytes, only one with
coalesced IDs, and still includes the helper body plus its call occurrence while
omitting the selected target body. These are byte measurements, not tokenizer
costs. No source warnings are reported for either case. This evidence rules out
exact duplicates as the dominant remaining cause; investigate admission between
ranked seed evidence and graph-discovered evidence, retaining navigation facts.
