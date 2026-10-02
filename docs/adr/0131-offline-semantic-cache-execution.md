# ADR-0131: Explicit offline semantic cache execution

- Status: accepted
- Date: 2026-09-30

## Decision

Add `--semantic-offline` to the Rust CLI. It requires `--semantic` and an
explicit `--semantic-model-revision`, and conflicts with `--allow-network`.
Endpoint, model, revision, prompt, configuration, and credential identity remain
unchanged so online and offline runs address the same durable cache entries.
Offline mode is an execution policy, not a different semantic configuration.

Reuse Penelope's existing cache-before-provider lookup and retryable jobs;
do not add another cache or snapshot format. Block inference at the HTTP send
boundary and model metadata discovery before any request. A missing cache entry
fails the semantic step without partial semantic publication. Deterministic
indexing remains independent and completed entries remain reusable on retry.
Remote HTTPS configuration may be inspected offline without network permission.
This does not disable unrelated Git/filesystem operations or make caller-asserted
model revisions verified identities.

## Verification

Exercise the CLI with a listening provider: reuse cached documents offline with
zero incoming requests; change a document and prove the cache miss makes no
requests or new semantic publication and retains historical semantic facts.
Deterministic reindexing still invalidates facts grounded in edited source;
offline failure does not promise stale facts remain current. Check option conflicts and
metadata/inference guards, then run the contributor gate.

Verified on 2026-09-30: the CLI fixture passes offline cache reuse, changed-source
cache miss, retained historical semantics, and subsequent online retry. Host
tests guard both HTTP paths and invalid option combinations. `cargo make ci`
passed, including strict Clippy and architecture dependency checks.
