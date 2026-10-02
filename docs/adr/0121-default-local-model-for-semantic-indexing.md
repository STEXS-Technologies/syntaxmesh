# ADR-0121: Default the one-command semantic indexer to a local model

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0119 provides a single opt-in CLI command for deterministic indexing plus
AI-assisted documentation enrichment. The CLI help advertises `--semantic`
without an argument, but the parser requires a model name, so the advertised
shortcut fails. README already uses `qwen3:latest` as its local Ollama example.

Graphify's documented AI workflow groups uncached docs into parallel batches
and caches extracted results per file. SyntaxMesh now follows the useful
performance shape with independently cached document requests packed into
bounded multi-document prompts, and publishes validated semantic facts in one
generation. The remaining simple UX improvement is to make the existing local
default work as the CLI help promises.

## Decision

1. Make `--semantic` alone opt in to the local Ollama-compatible model name
   `qwen3:latest` at the existing loopback endpoint. Keep
   `--semantic <model>` and `--semantic=<model>` as explicit overrides.
2. Keep semantic enrichment opt-in. Indexing without `--semantic` never
   creates a provider or performs model/network requests.
3. Do not download model weights, start a model server, or select a remote
   provider automatically. The local Ollama service and selected model must
   already be installed/available. Remote HTTPS remains explicit and requires
   `--allow-network`.
4. Keep request batching, per-document Penelope caching, provenance, evidence
   validation, and publication semantics unchanged.

## Consequences

- The documented one-command path can omit the model argument:
  `syntaxmesh index . syntaxmesh.snapshot --semantic`.
- Users with another installed model can select it explicitly.
- The shortcut does not make first-time model installation or service startup
  automatic; those remain visible host operations rather than hidden side
  effects.

## Verification

- CLI integration exercises bare `--semantic` against a loopback mock and
  verifies the selected model in the request.
- Existing explicit-model integration coverage continues to exercise overrides
  and remote endpoint policy.

## References

- [ADR-0119: One-command parallel semantic indexing](0119-one-command-parallel-semantic-indexing.md)
- [ADR-0120: Per-document cache with multi-document prompts](0120-per-document-cache-with-multi-document-prompts.md)
- [Graphify extraction workflow](https://github.com/Graphify-Labs/graphify/blob/v8/docs/how-it-works.md)
