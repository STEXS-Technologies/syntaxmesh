# ADR-0147: Bound the serialized semantic user prompt

- Status: accepted
- Date: 2026-09-30

## Decision

Reuse the exact-serialization budgeting approach from the Rust MCP context host:
the CLI semantic packer counts the actual UTF-8 JSON string delivered as the user
message, including source escaping, heading arrays, evidence hashes, keys,
request wrappers, and separators. Use the same encoding helper for planning and
execution. Preserve the existing wire shape and 48 KiB user-message cap.

Keep source request construction conservatively bounded to 32 KiB of raw text
and headings, leaving normal JSON overhead room. This is not sufficient for all
escaping patterns: reject any indivisible request exceeding the exact cap before
inference HTTP instead of truncating source. Retain the 32-chunk, 22-request, four-worker,
and bounded recovery limits. Failed requests remain retryable.

The cap covers the decoded user message, not the outer HTTP envelope, system
prompt, output budget, or a provider-specific token limit. No tokenizer guarantee
or model context-window compatibility is claimed. No new dependency, runtime,
cache schema, public DTO, or provider payload fields are introduced. Smaller
packing may create more prompts; no latency superiority is claimed.

## Verification

Compare the helper's output with the previous serde JSON representation for
Unicode, quotes, backslashes, controls, headings, and multiple requests. Verify
exact envelope/separator accounting and bounds before HTTP. Existing File/Turso
semantic, cross-document, cache, and failure fixtures must remain green.

The exact-encoding regression matches the previous serde representation for
Unicode and escaped source/heading values. A raw 26 KiB quote-only request is
rejected with zero HTTP attempts through both single and batched entry points.
Two raw 15 KiB escaped sources are delivered in separate prompts, each verified
under the actual user-message byte cap by the loopback fixture.
