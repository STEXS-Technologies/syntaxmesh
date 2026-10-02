# ADR-0119: Run bounded semantic indexing in one explicit CLI command

- Status: accepted
- Date: 2026-09-29

## Context

SyntaxMesh already has deterministic documentation indexing, source-grounded
semantic facts, authored section context, a generation-wide bounded request
builder, and a Penelope content cache. Applications must still provide a model
adapter and orchestrate requests/publication themselves. Graphify's useful
workflow is one command that batches uncached documents and runs semantic work
in parallel; its structural code pass remains separate and local.

## Decision

1. Add one opt-in CLI path: `syntaxmesh index <root> <snapshot> --semantic
   <model>`. It indexes deterministic facts first, then semantically enriches
   the same accepted generation. Without `--semantic`, no provider is created
   and no network/model request can occur. The default endpoint is the local
   OpenAI-compatible Ollama endpoint at `http://127.0.0.1:11434/v1`.
2. Permit an alternate endpoint only through an explicit CLI option. Loopback
   endpoints remain local; non-loopback endpoints require `--allow-network`,
   HTTPS, and credentials supplied by an environment-variable name/value, not
   embedded in source or printed command output. No endpoint is selected by
   API-key presence alone.
3. Keep HTTP and model-specific JSON handling in the CLI host adapter. Core,
   semantic DTO, engine, language, and thin protocol crates gain no transport
   dependency. Use OpenAI-compatible Chat Completions with JSON-only structured
   output so local Ollama and explicitly enabled compatible hosts share one
   adapter.
4. Build a deterministic sequence of requests from the accepted generation,
   bounded by chunk count and UTF-8 bytes. Penelope durably prepares every
   request before provider execution. Providers may override a default
   sequential `extract_many` method for bounded parallel execution. Validate
   each result against exact quoted source chunks, then merge all batches and
   publish one graph delta only after every batch succeeds. A failed run leaves
   deterministic indexing intact and semantic work retryable; completed
   request outputs remain reusable on retry.
5. Semantic publication replaces the current facts owned by the dedicated
   `syntaxmesh.semantic` namespace atomically with the new validated result.
   Historical generations retain the replaced facts. Re-running identical
   inputs/configuration is a no-op when the current semantic fact set matches.
6. Report batch count, provider calls/cache reuse, and published fact counts.
   Usage/cost telemetry beyond counts remains deferred until provider adapters
   can report it portably.

## Consequences

- A configured local model can build the documentation concept/claim graph
  with one command; remote inference requires a separate explicit opt-in.
- Bounded parallel calls reduce wall time while Penelope preserves restartable
  cache/job state. Publication is atomic across semantic batches rather than
  exposing a partially enriched current graph.
- Model interpretation remains an inference, not an authored source fact;
  every accepted claim continues to cite exact chunk text and retain
  `SemanticInference` provenance.
- Cross-batch reasoning is not implied. Chunk/byte limits are conservative
  context bounds, and claims can cite only chunks present in their request.
- CLI is the host boundary. No provider SDK, network access, or model runtime
  enters core, Engine, language packs, or semantic contracts.

## Verification

- Existing semantic and Penelope tests cover exact evidence rebinding,
  restartable prepared jobs, configuration-sensitive cache identity, and
  bounded cross-document request construction.
- A CLI integration test runs source indexing and semantic publication in one
  invocation against a loopback mock endpoint and verifies the request's exact
  chunk hash is accepted into the semantic fact batch. A second invocation
  with the same source and provider configuration succeeds with the mock
  server closed, reports cache reuse, and creates no additional generation.
- Compile and targeted engine, Penelope, CLI, and integration suites pass.
- Additional regressions still needed before this ADR's full intended gate:
  explicit remote-policy tests, a multi-request concurrency-order test,
  retry/partial-failure batch coverage, semantic replacement/history coverage,
  and architecture dependency assertions.
- A real local model has not been used for quality or latency evaluation; the
  mock validates plumbing and grounding, not model quality.

## References

- [Graphify how it works](https://github.com/Graphify-Labs/graphify/blob/v8/docs/how-it-works.md)
- [Graphify extraction workflow](https://github.com/Graphify-Labs/graphify/blob/v8/graphify/skill-trae.md)
- [ADR-0115: Source-grounded document semantic facts](0115-source-grounded-document-semantic-facts.md)
- [ADR-0116: Penelope semantic enrichment cache](0116-penelope-semantic-enrichment-cache.md)
- [ADR-0118: Bounded generation semantic batches](0118-bounded-generation-semantic-batches.md)
