# ADR-0127: Preserve independent requests in batched semantic prompts

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0120 independently caches document requests and combines misses into
bounded prompts. Every accepted claim must be supported entirely by one
request. The CLI currently flattens the combined request into a single chunk
array, leaving the model unable to identify the boundaries it must respect.
Authored headings cannot reliably identify a document or a cache partition.

## Decision

1. Send a `requests` array to the model, with one nested `chunks` array per
   independently cached request. Preserve the bounded multi-document call and
   exact chunk hashes, text, and authored heading context.
2. Tell the model that every claim must cite evidence within one array entry.
   Neighboring entries remain contextual input; evidence validation continues
   to reject claims crossing independent cache identities.
3. Increment the host prompt version and hash so previously cached output is
   not reused under the changed extraction prompt.
4. Keep this representation in the Rust CLI host. No public semantic DTO,
   store schema, transport protocol, or canonical fact format changes.

## Consequences

The provider sees the same independence boundary used by durable caching and
evidence validation. This removes an avoidable ambiguity; model quality and
latency still require evaluation with a real provider. Cross-document
synthesis remains a separate feature requiring dependency-aware cache identity.

## Verification

The one-command loopback integration fixture checks two separate request groups
in the first packed prompt, one group after one document changes, and zero
provider calls for an unchanged repeat. Existing exact-evidence validation and
atomic semantic publication continue to apply.

The complete local Linux `cargo make ci` gate passed after this change and the
HTTP policy follow-up in ADR-0128.

## References

- [ADR-0120: Per-document cache with multi-document prompts](0120-per-document-cache-with-multi-document-prompts.md)
- [ADR-0119: One-command semantic indexing](0119-one-command-parallel-semantic-indexing.md)
