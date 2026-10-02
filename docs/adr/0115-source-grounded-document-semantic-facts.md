# ADR-0115: Add source-grounded document semantic facts

- Status: accepted
- Date: 2026-09-29

## Context

SyntaxMesh deterministically stores Markdown/plain-text content as searchable
`DocumentChunk` facts, and explicit authored links can create source-grounded
relationships. It does not yet build a concept/claim graph from the meaning of
that content. Roadmap §§27–29 reserve model-derived enrichment for a separate
`syntaxmesh-semantic` boundary, with distinct provenance, cache identity,
failure isolation, resumability, optional networking, and local-model support.

The existing extension fact envelope can publish namespaced analysis facts
through Penelope and the normal generation/history path. `NodeKind::External`,
`RelationKind::External`, and `EvidenceClass::SemanticInference` already
represent this distinction without adding core node/relation variants or a
second persistence model.

## Decision

1. Add a separate Rust `syntaxmesh-semantic` crate. It depends on the core and
   extension fact contracts, but not on stores, Penelope, hosts, transports,
   or model SDKs. Deterministic extraction and ordinary code indexing require
   no semantic provider and no network access.
2. Represent extracted ideas as stable, namespaced external `concept` nodes;
   represent a subject/relation/object assertion as a stable external `claim`
   node with typed-in-namespace `subject`, `object`, and `supported_by` edges.
   A source `DocumentChunk` has a `supports` edge to the claim. These reuse
   existing external fact labels and publish through `FactBatch` with the
   `AnalysisFacts` capability.
3. Every claim must cite at least one exact input `DocumentChunk` and a
   non-empty quote that is an exact substring of that chunk. Resolve its byte
   span relative to the chunk's source span, using checked offset arithmetic.
   Reject missing chunks, non-document nodes, stale/malformed source spans,
   quotes not present verbatim, unreferenced concepts, and malformed relation
   labels. Provider text must equal the indexed chunk payload, and the chunk's
   owning file must agree with its source location. Recompute request/cache
   fingerprints after deserialization before provider execution. Never silently
   drop invalid model output or invent code targets.
4. All generated concepts, claims, and edges use `EvidenceClass::SemanticInference`
   and producer provenance bound to provider identity and source evidence.
   They remain distinguishable from deterministic parser facts and are
   retractable/replaced through ordinary extension fact publication.
5. The cache identity is a stable digest over provider, model, model revision,
   prompt hash/version, provider configuration hash, and canonical input-chunk
   identities/content hashes. Equal content under equal semantic configuration
   can therefore be reused across snapshots; changing any input component
   invalidates the result.
6. This ADR defines the provider-neutral fact contract and evidence validation
   only. It does not add an HTTP provider, choose a hosted model, enable network
   access, persist cache/job state, or make semantic extraction part of normal
   `index`. Those require separate optional adapters/orchestration and must
   preserve disable-network, local-model, resumability, and failure-isolation
   requirements. No public core DTO, canonical storage schema, or NDJSON
   contract changes in this slice.

## Alternatives considered

- Add concepts to deterministic Markdown indexing by matching words or
  headings: rejected because lexical coincidence is not semantic evidence.
- Store model output only as summaries in a side database or JSON/NDJSON file:
  rejected because it would not be a generation-scoped navigable graph and
  would duplicate persistence/query semantics.
- Add dedicated core `Concept`/`Claim` variants immediately: deferred because
  the namespaced external-fact path already carries these facts and provenance
  without freezing a core ontology before its contract is ready.
- Publish an inferred relationship without its source chunk: rejected because
  an agent must be able to inspect the exact authored evidence behind a claim.

## Consequences

- Agents can traverse document chunks to semantic claims and concepts while
  seeing that those edges are inferred and locating the exact supporting text.
- A provider is replaceable; changing provider/model/prompt/config cannot
  silently reuse stale semantic output.
- This does not itself make semantic extraction user-facing: provider
  implementation, cache/job persistence, resumable orchestration, query
  affordances, and end-to-end local/remote opt-in remain future work.

## Verification

- Unit fixtures prove stable IDs/cache identity, exact quote-to-byte-span
  mapping, multi-document support edges, and rejection of missing or altered
  evidence.
- Extension SDK validation accepts the namespaced semantic batch and retains
  `SemanticInference` provenance.
- Engine integration publishes the batch through Penelope, queries concepts
  and claims at the accepted generation, and proves semantic facts do not
  appear in the prior generation or mutate deterministic source facts.
- Workspace architecture checks prove the semantic crate has no store,
  workflow, transport, or host dependencies.

## References

- [Roadmap §§27–29](../syntaxmesh-todo-extensible-opensource-v5.md)
- [ADR-0111: Source-grounded document chunks](0111-source-grounded-document-chunks.md)
- [ADR-0112: Source-grounded rationale links](0112-source-grounded-rationale-links.md)
- [ADR-0014: Engine extension fact ingestion](0014-engine-extension-fact-ingestion.md)
