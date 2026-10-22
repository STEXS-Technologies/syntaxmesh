# ADR-0348: Exact-premise seeded inference discovery

Status: pure seeded discovery and canonical Query range admission implemented;
incremental publication and Penelope synchronization checkpoints pending.

## Decision

Premise closure invalidation cannot discover newly enabled witnesses. Add a pure,
bounded discovery primitive for the existing deterministic path/join/declaration
rule language. Its input is one authoritative active selection, a rule set and a
set of exact active edge-version references opened in an accepted transition
range. Query, not the pure evaluator, verifies that range and its generation
coordinates. A configuration change requires explicit complete rebuilding;
canonical premise seeds do not represent changed rules or ontology semantics.

Every returned claim must contain at least one supplied exact premise version.
Missing, stale, non-edge or duplicate seed references reject rather than silently
produce incomplete results. An explicitly empty seed set produces no claims after
ordinary selection/rule admission; it does not mean a complete empty selection.

Build bounded reverse adjacency and derive candidate source nodes per rule from
the seeded edge's position in the witness language. A path seed permits its
source and path ancestors; a join seed permits nonempty path ancestors of its
source; a declaration seed permits path ancestors of matching incoming join
sources, and must target the rule's concept. Forward traversal then uses the same
acyclic witness construction and identity as complete evaluation, emitting only
seed-bearing witnesses. All candidate walks and forward edge visits consume one
shared work budget; candidate depth is bounded by the existing path-depth limit.
Deduplicate candidate sources and emitted witnesses so multiple seeds do not
duplicate output. Bounds fail the whole request, never return partial success.

The primitive must match complete evaluation filtered by exact seed membership
for every supported rule, including overlapping relation kinds, cycles, multiple
seeds, multiple rules and restored identities with new versions. Adding an edge
may enable a witness containing that edge; node additions alone cannot enable
this rule language without an active edge witness. This reasoning does not apply
to future negative conditions or recursive derived-claim rules.

## Publication and runtime boundaries

The Query adapter exposes `infer_discovered_rules` using a pure `DiscoveryRange`
(`from`, positive generation and version budgets), the Query's pinned target and
ordinary rule/history/evaluation controls. Share accepted-transition validation
with invalidation; collect exact openings and closures, discard versions closed
by the target, and seed only edge versions. Same-generation ranges are empty but
still validate selection/rules. Reversed, foreign or malformed ranges reject
before evaluation. The adapter still loads a bounded authoritative selection.

Discovery output is a delta of witnesses, not a complete `Watermark`. Do not reuse
complete-selection publication or declare other witnesses absent. Persisted
incremental claim sets, atomic discovery/invalidation checkpoints and Penelope
recovery require their own subsequent contracts. Existing rebuild/lifecycle
coordinators and canonical history remain unchanged.

The pure crate remains free of storage, workflow, transport and host dependencies.
An initial Query adapter may still load a bounded authoritative selection; seeded
traversal alone does not establish indexed input loading or O(1) query cost.
Materialized temporal reads must remain separate from discovery computation.

## Required evidence

Compare seeded discovery with filtered complete evaluation for seeds in all
three witness positions, irrelevant seeds, restored edge versions, multi-seed
deduplication, cycles and rule-order independence. Verify malformed seeds,
selection/rule admission and every exhausted bound reject. Include an unrelated
graph component that complete evaluation traverses but candidate traversal avoids.
Run strict Clippy and the pure dependency boundary gate before exposing the API.

## Implementation evidence

`syntaxmesh_inference::discover_rules` admits exact active seeds, builds reverse
candidate sources and reuses complete evaluation's witness construction/identity.
Selection/rule validation also runs when seeds are empty. The total encoded input
bound includes seed references; reverse construction, seed/rule checks and walks
share the forward traversal work budget. Candidate walks reject depth exhaustion
instead of silently truncating possible affected ancestors.

Twenty-five pure unit tests and the Rust-source integration fixture pass with
strict Clippy. Seven discovery fixtures cover exact byte boundaries, zero and
exhausted limits, empty/foreign/stale/duplicate seeds, restored versions, input
and rule order, cycles, multi-seed deduplication and unrelated components. A
differential fixture compares every active seed on all 512 directed three-node
graphs, including self-loops and overlapping path/join/declaration relations,
with filtered complete evaluation. This establishes the tested rule language,
not arbitrary recursive inference. The preceding full contributor CI passed
before this discovery slice; latest verification is targeted.

The public pure API trusts its caller's active selection, just like complete
evaluation. It does not certify that seeds were opened in a specific canonical
range. `Query::infer_discovered_rules` now provides that range admission using
the shared accepted-generation/version-feed verifier and bounded authoritative
selection loading. Four Query tests and two SQL history fixtures pass across
InMemory, File, SQLite and Turso: creation, opened-then-closed exclusion,
restoration, same-generation emptiness, reversed ranges and exhausted range bounds.
The shared verifier also rejects the existing fourteen malformed feed cases.
Strict feature-enabled Query/inference Clippy passes. Atomic durable
synchronization is still required before a full-engine incremental command.
