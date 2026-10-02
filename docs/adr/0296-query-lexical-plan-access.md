# ADR-0296: Expose indexed lexical composition through the pinned Query service

Status: accepted for additive opt-in implementation; default host routing unchanged.

The MCP host accesses canonical reads through its generation-pinned `Query`
service, not a raw store handle. ADR-0295's indexed composer must be usable through
that boundary without exposing stores or duplicating ranking in the host.

Add thin `Query` methods for building the ADR-0294 planner-capable identifier
index and invoking its lexical seed and complete packing plans. Delegate to the
existing index with the service's store and generation. Preserve all index
identity, budget, membership and capacity validation; introduce no new ranking,
hydration, graph traversal, runtime dependency or automatic routing behavior.

Use these methods in the existing opt-in family retrieval diagnostic. Build the
planner-capable index instead of the plain identifier index for that diagnostic.
Compare indexed seed order and complete packing order against the independently
retained canonical corpus oracle before using each result. Fail on disagreement;
do not substitute oracle output after an indexed failure. Give the indexed policy
a distinct artifact identifier and report composition timing separately from
oracle verification. Existing unrelated diagnostic policies remain unchanged.

Require Query/MCP tests, strict Clippy and contributor gates, then representative
whole-Sim and whole-Shardline retrieval evidence. Existing candidate/token bounds,
source verification and exact target-node admission gates remain authoritative.
Diagnostic success alone does not authorize default production routing or prove
replacement of Graphify.

The thin Query adapter and opt-in indexed diagnostic are implemented. All 57
Query tests, 23 MCP tests and strict Query/MCP Clippy pass. Restricted-port
coverage confirms Query delegation preserves ordering, aggregate/build budgets,
manifest validation and no payload hydration.

Selected-file Sim run `context-20261002T090300.307836Z-uncommitted` completes
successfully in 17.71 seconds: five target admissions and five exact seed/packing
oracle comparisons pass. Planner index construction takes 332,208 microseconds;
individual seed/packing composition takes 1,983–2,861 microseconds, excluding
oracle verification. These are single-sample small-fixture diagnostic timings,
not an end-to-end production latency claim. Whole-Sim indexed evaluation is
running. Whole-Shardline evidence and contributor CI for this integration remain
required; default routing is unchanged.

Full contributor CI for this integration passes in 210.39 seconds with two
build jobs, including all 17 backend conformance scenarios and documentation.
Whole-Sim indexed retrieval is still running; representative quality is not
inferred from contributor CI or the selected-file success.

Whole-Sim `context-20261002T090339.373794Z-uncommitted` finishes with all
five indexed seed/packing oracle comparisons and all five diagnostic target
admissions passing. Corpus fingerprint remains
`d085ad193619f9cb5957280fc2d47f93562a3ea6156c0baa69ecaf1710c91d24`
over 2,572 files. Publication takes 410.490 seconds; index construction takes
78.094302 seconds. Individual indexed compositions take 2.504–29.369 ms,
excluding graph selection, hydration, packing and oracle verification. The
benchmark command exits unsuccessfully because unchanged default retrieval
remains 3/10 and fails the required-target gate, not because indexed equivalence
or admission fails. Whole-Shardline regression is running; no default promotion.

Current whole-Shardline run `context-20261002T091538.461442Z-uncommitted`
prepares 828 files with fingerprint
`7c9f2991a82a1dce2461ff1633bee73cc30956c321cd10877ee5c3eba1c0ba81`.
This differs from ADR-0293's 810-file corpus. Treat its eventual result as
current-repository validation, not an unchanged-input timing comparison or proof
that every earlier fixture fact is identical. The run is still publishing.

The current whole-Shardline run finishes with five exact seed/packing oracle
comparisons and five diagnostic target admissions passing, retaining Rust
webhook, Python metadata, Bash and both documentation targets. Publication is
972.347 seconds; planner index construction is 273.484769 seconds. Individual
indexed compositions take 13.434–91.534 ms, excluding selection, hydration,
packing and oracle work. Unchanged default retrieval is 6/10 and misses Python
metadata, so the benchmark command still exits unsuccessfully. Combined with
whole-Sim and contributor CI, this closes representative diagnostic equivalence
and admission evidence for this integration, not default host routing,
generation-aware host caching, production end-to-end performance or broader
retrieval completeness. No source-of-truth requirement is waived.
