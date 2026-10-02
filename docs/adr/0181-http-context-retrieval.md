# ADR-0181: Bounded HTTP context retrieval

Status: Accepted

## Decision

Add POST `/api/v1/context` using the existing current/historical query compiler
and ADR-0180 host adapters. Enable source access explicitly with the host builder
`with_context(root, tokenizer)` or CLI suffix `--context <root> <tokenizer>`.
Without configuration, valid context requests return 503. Supported encodings
are `cl100k_base` and `o200k_base`; this is retrieval, not model inference.

Accept one JSON object, rejecting unknown fields: `query`, `token_budget`, and
optional `generation`, `seed_nodes`, `max_hops`, `max_candidates`. Bound the
body to 64 KiB, query to 4096 UTF-8 bytes, seeds to 64, token budget to 1–32768,
hops to 0–8 (default 1), and candidates to 1–256 (default 64). IDs use 64 hex
characters. Invalid input returns 400, wrong content type 415, oversized body
or an unfit minimum context pack 413, missing seeds/generations 404.

Return the existing schema-2 ContextPack directly as compact JSON, not inside
the schema-1 HTTP envelope: the token budget accounts for the entire successful
JSON body, excluding headers. Failures retain the existing redacted HTTP error
envelope. Reuse blocking-job admission, repository/worktree scope checks and
startup generation checks before/after queries. No NDJSON, inference, mutation,
history replay, new storage schema, or alternative retrieval algorithm.

Historical source evidence must match the indexed content hash; the filesystem
reader does not reconstruct deleted or changed historical bytes. Existing
compiler warnings/omissions remain authoritative. Loopback trusted-process
restrictions still apply; authentication, refresh/watch, bounded drain, and
complete daemon acceptance remain open.

## Verification

Real TCP fixtures compare current and retained-generation packs with independent
Engine queries for both supported tokenizers, verify compact whole-body token
accounting, and exercise invalid fields/IDs/caps, missing seeds/generations,
tiny budgets, body/content-type rejection, Origin restrictions, absent source
configuration, admission exhaustion, and stale startup generations. Historical
fixtures use changed filesystem bytes rather than substituting current text as
old evidence. Existing MCP and backend context conformance tests remain green.
Full `cargo make ci` completed successfully on 2026-09-30 in 131.69 seconds.

Documentation follow-up: composite Rust/Markdown indexing and reopened host
queries now verify actual decision rationale and line spans over TCP for both
tokenizers. Current bytes and restored historic bytes yield source evidence;
changed/deleted files yield `stale_source`/`source_unavailable` and no source
quotation, while retained graph facts remain readable. Full CI passed in
126.72 seconds. No production contract or archival capability changed.
