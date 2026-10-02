# ADR-0124: Default the index command to the current project

- Status: accepted
- Date: 2026-09-29

## Context

The Rust CLI already performs deterministic indexing and optional AI document
enrichment in one invocation. However, the common path requires users to pass
both the project root and a snapshot path. This is avoidable workflow friction
for local-first use. Graphify's useful interaction model is to run one command
at the project root; its documented pipeline keeps structural extraction
deterministic, performs semantic extraction only for content documents, caches
unchanged files, and parallelizes bounded document batches.

SyntaxMesh already has deterministic multi-language indexing, opt-in semantic
enrichment, bounded multi-document prompts, parallel provider requests,
per-document durable cache entries, exact evidence validation, and atomic
history-preserving semantic publication. Preserve those behaviors and the
provider-free default; simplify only the CLI's common path.

## Decision

1. `syntaxmesh index` defaults the source root to the current working
   directory and the snapshot to `<root>/.syntaxmesh/index.snapshot`.
2. `syntaxmesh index <root>` uses the same default snapshot under that root.
   Existing `syntaxmesh index <root> <snapshot>` remains unchanged.
3. Create the default `.syntaxmesh` directory before opening the store. The
   snapshot remains a local FileGraphStore reference backend; this does not
   change the production-store or migration contracts.
4. AI remains explicit: `syntaxmesh index --semantic` opts into the configured
   local Ollama-compatible endpoint and documented default model. The command
   does not download weights, start a model server, contact a remote service,
   or read provider credentials unless the corresponding explicit options are
   supplied.
5. `index-turso` continues to require explicit source-root and database paths.

## Consequences

- From a project root, `syntaxmesh index --semantic` performs the deterministic
  and semantic passes in one command; `syntaxmesh index` remains fully local
  and provider-free.
- Users can still select an explicit root and/or snapshot using the positional
  compatibility forms.
- Model installation/service availability remains a visible prerequisite for
  AI enrichment. The command remains Rust-hosted; it does not launch analyzed
  language runtimes.

## Verification

- CLI integration runs `index --semantic` with the current directory as the
  project root and the default project-local snapshot, against a loopback
  provider fixture.
- The same fixture verifies unchanged document cache reuse and one-document
  invalidation without additional commands or loss of history.
- CLI parsing coverage preserves explicit root/snapshot and `index-turso`
  behavior; the full `cargo make ci` gate remains required.

## References

- [Graphify how it works](https://github.com/Graphify-Labs/graphify/blob/v8/docs/how-it-works.md)
- [ADR-0119: One-command semantic indexing](0119-one-command-parallel-semantic-indexing.md)
- [ADR-0120: Per-document cache with multi-document prompts](0120-per-document-cache-with-multi-document-prompts.md)
- [ADR-0121: Default local semantic model](0121-default-local-model-for-semantic-indexing.md)
