# ADR-0042: Explicit, evidence-backed consequence edges

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0036 requires a temporal lineage graph that can distinguish explicit or
derived consequences from mere event correlation. The current
`ChangeEventCorrelation` query correctly exposes same-fact co-change without
claiming causation, but there is not yet a model for cross-fact consequences.
The source-of-truth describes these edges conceptually as a typed relation
between change events or fact versions, with evidence, derivation, and
provenance. It explicitly forbids inferring causation from event order alone.

SyntaxMesh already has stable `ChangeEventId`, `FactVersionRef`, and
`ProvenanceId` types, an atomic `GraphDeltaWithLineage` publication boundary,
and indexed event/fact lookups. Extend that model rather than adding a second
history store or placing mutation planning in SyntaxMesh.

## Decision

1. A consequence edge has stable source and target endpoints, each either a
   `ChangeEventId` or an exact `FactVersionRef`. It has a typed consequence
   class, a non-empty set of exact `FactVersionRef` evidence, a derivation
   description, and the `ProvenanceId` of the producer asserting or deriving
   it. Evidence answers why the relation exists; provenance answers who or
   what supplied that assertion.
2. The initial class vocabulary follows source-of-truth §239: direct dependency
   effect, generated artifact effect, derived semantic effect, contract effect,
   build effect, test effect, deployment effect, runtime-observed effect,
   declared-migration effect, change dependency, and possible downstream
   effect. `HistoricallyCorrelated` remains the distinct non-causal
   `ChangeEventCorrelation` relation from ADR-0038; it is not stored as a
   consequence edge. `DirectChange` remains the `ChangeEvent`'s changed-fact
   membership, not a redundant self-edge.
3. The first producer-facing write contract accepts explicit edge assertions
   only. It never infers consequences from order, shared identity, or a graph
   path. A future deterministic derivation may be added only with a named
   derivation kind, exact evidence, and parent-edge references sufficient to
   explain its derivation. Derived edges may not cite only their own output.
4. An edge becomes valid at the accepted generation containing its assertion.
   It is immutable. Retraction, if later required, is a separate
   provenance-backed temporal assertion; it does not rewrite history.
   Generation distance is not stored because generation sequences are local to
   a store/repository and are not a cross-repository time unit. Queries return
   endpoint generations; distance may be derived only when both endpoints
   share an ordered generation history.
5. An assertion is published through the existing prepared-lineage workflow
   alongside its graph delta. SQLite and Turso persist the journal and
   source/target/evidence indexes atomically with the generation; InMemory
   and File remain reference implementations. The indexed query surface is
   bounded by selected depth and result size, and every traversal has explicit
   depth and result limits.
6. `ChangeEventCorrelation` and consequence edges use distinct public query
   modes and DTO discriminants. No API reports a generic `caused_by` edge
   without carrying its typed class, evidence, derivation, and provenance.

## Alternatives considered

- **Infer causation from event ordering or co-change:** rejected by ADR-0036's
  causality discipline.
- **Treat the normal code dependency graph as the consequence graph:** rejected;
  entity dependency and change-to-change dependency have different endpoint
  identities and temporal meanings.
- **Use `ChangeEventCorrelation` for every relation:** rejected; shared-fact
  correlation is useful but cannot represent cross-fact evidence or stronger
  declared dependency classes.
- **Add a separate mutable lineage database:** rejected; the canonical store
  and existing publication transaction remain authoritative.
- **Store generation distance on the edge:** rejected because repositories
  have independent generation sequences and generation IDs are not dates.

## Consequences

- Core DTOs and store ports need typed endpoints, kinds, derivation, evidence,
  provenance, and bounded traversal contracts before publishing this feature.
- Durable adapters need additive schema migrations and backfill fixtures;
  legacy history remains without inferred consequence edges.
- Penelope's durable prepared record must bind assertions into its digest and
  retry checks. StateChronicle continues to verify accepted generation history
  and is not repurposed as the lineage query database.
- Product readiness still requires separate bitemporal state selection,
  representative storage/write-amplification measurements, and cross-backend
  publication/restart tests.
