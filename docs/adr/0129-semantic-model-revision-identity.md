# ADR-0129: Identify model revisions before semantic cache lookup

- Status: accepted
- Date: 2026-09-30

## Context

The semantic contract and Penelope cache already bind results to model revision.
The CLI currently substitutes the configured model name, so changed weights
behind a mutable tag such as `latest` reuse prior outputs. Ollama's existing
`GET /api/tags` API reports model digests; other OpenAI-compatible providers do
not expose one universal immutable-revision API.

## Decision

1. Before cache lookup, discover the selected local Ollama model's digest and
   use it as `model_revision`. Resolve a missing tag as `latest`, require exactly
   one matching model and a valid SHA-256 digest, and fail on unavailable or
   malformed metadata rather than falling back to the mutable model name.
2. Recheck discovered revision after each inference call before its output is
   cached, and before semantic publication. Fail if the server reports a
   changed revision. These are server-reported consistency checks, not remote
   attestation or atomic model pinning against adversarial/concurrent swaps.
3. Add `--semantic-model-revision <revision>` for an explicit caller-asserted
   immutable revision. This skips metadata requests and supports offline cache
   reuse. The caller must change it when the provider/model changes; SyntaxMesh
   does not verify this assertion. Distinguish asserted/discovered provenance.
4. Automatic metadata discovery is limited to loopback Ollama-compatible
   `/v1` endpoints. Other providers require an explicit revision. Existing
   remote authorization, HTTPS, redirect, proxy, and credential policies apply.
5. Keep discovery in the CLI host and reuse existing semantic identities,
   Penelope records, evidence validation, and history publication. No core
   contract, store schema, model download, or model-service startup is added.

## Consequences

`syntaxmesh index --semantic` stays one command with local digest discovery.
Discovery requires the local service on every run, including cache hits; only
explicitly asserted revisions allow offline cache reuse. Initial inferred
results under the old alias-only identity are not reused. Metadata calls add
small HTTP overhead, while unchanged documents still avoid inference calls.
The CLI reports inference and metadata request counts separately.

## Verification

Fixtures must cover digest changes, unchanged revisions, missing/duplicate/
malformed model metadata, explicit offline revisions, and changed revision
rejection before cached output/publication. Existing one-command integration,
history, dependency boundaries, and strict contributor checks remain required.

Five dedicated host tests now cover the above revision/option cases, including
a changed-model completion remaining retryable through the real Penelope
enricher. The one-command integration fixture passes with an asserted revision,
including offline cache reuse and document-edit/history behavior. The full local
Linux `cargo make ci` gate passed after these changes. No real model-quality,
latency, or provider-attestation claim follows from these mock fixtures.

A subsequent CLI process fixture uses automatic discovery against a loopback
Ollama-shaped server. It proves initial publication, unchanged cache reuse
without a new generation, changed-digest re-inference with retained historical
producer provenance, and rejection of a final pre-publication revision change
without altering the previously accepted semantic graph. It also checks the
reported inference/metadata request counts. See
`crates/syntaxmesh-cli/tests/semantic_revisions.rs`.

## References

- [Ollama model-list API](https://docs.ollama.com/api/tags)
- [ADR-0116: Penelope semantic cache](0116-penelope-semantic-enrichment-cache.md)
- [ADR-0128: Semantic HTTP policy](0128-semantic-http-endpoint-policy.md)
