# ADR-0116: Persist semantic enrichment through Penelope

- Status: accepted
- Date: 2026-09-29

## Context

[ADR-0115](0115-source-grounded-document-semantic-facts.md) defines a
provider-neutral source-evidence contract, but deliberately does not persist
cache entries or jobs. Roadmap §§28–29 require a content-addressed semantic
cache and resumable jobs. The full engine already uses Penelope for durable
multi-step operations, and graph stores expose a durable-record CAS/page port.
The Penelope analytics exporter demonstrates preparation-before-side-effect,
replay-validated saga records, CAS completion, and bounded restart recovery.

## Decision

1. Add a Penelope-managed semantic enrichment adapter to
   `syntaxmesh-integration-penelope`; depend on the existing `SemanticProvider`,
   `SemanticRequest`, and `SemanticOutput` contract. Do not add model/network
   dependencies to core, engine, language packs, or the semantic contract crate.
2. Use the existing `DurableRecordStore` for prepared/completed process records.
   The operation/cache key is the semantic request cache key, which already
   binds provider, model/revision, prompt/version/hash, configuration hash, and
   content-only input identity. The store remains the only durable database.
3. Persist a prepared Penelope record before calling the provider. On provider
   or evidence-validation failure, leave it prepared and retryable. On success,
   persist the validated structured output and completed saga through CAS.
   Completed output is reusable across physical nodes/paths with identical
   semantic inputs; `SemanticRequest::into_fact_batch` rebinds it to each
   generation's source chunks and revalidates exact quotes.
4. Keep the full request only while prepared so restart recovery can retry it;
   compact it out of completed cache records. Recover prepared records in
   bounded durable-record pages. Recovery completes the cache/job record; graph
   publication remains an explicit caller operation through the ordinary
   extension fact workflow.
5. This adapter does not choose or enable any network provider, and it does not
   make semantic processing part of deterministic indexing. Applications opt
   into a provider and invoke the adapter explicitly. Local providers use the
   same trait without a transport dependency. Cost accounting and a CLI/MCP
   surface remain separate work.
6. The Engine assembles per-file requests from a pinned generation's existing
   `nodes_for_file` index and `DocumentChunk` payloads. It does not rescan or
   reparse documents, and an empty/non-document file is rejected before any
   provider call.

## Consequences

- Semantic computation survives process restart and can be retried without
  re-running successfully completed content/configuration pairs.
- Output remains generation-neutral in cache storage; evidence is checked and
  bound to the current physical source facts before graph publication.
- A changed source content or semantic provider configuration gets a different
  cache key. The adapter cannot silently serve an entry under a stale model or
  prompt identity.
- The store's durable-record port holds semantic process/cache records, so no
  parallel SQLite schema or NDJSON communication/storage format is introduced.

## Verification

- A failure-injection test leaves a prepared operation durable; after reopening
  the store, recovery retries and completes it.
- A completed result is reused for identical content under different physical
  node IDs/source paths without invoking the provider again, then rebound to
  the new source span and published as ordinary extension facts.
- Changed provider/model/configuration or input content does not hit the old
  cache entry.
- Bounded recovery is restart-safe, and every persisted Penelope event replays
  to a state consistent with its record phase.
- Workspace architecture checks preserve the dependency direction: semantic
  contracts remain free of database/workflow/transport/host dependencies.

## References

- [SyntaxMesh §§27–29](../syntaxmesh-todo-extensible-opensource-v5.md)
- [ADR-0115: Source-grounded document semantic facts](0115-source-grounded-document-semantic-facts.md)
- `PenelopeAnalyticsExporter` in `syntaxmesh-integration-penelope`
