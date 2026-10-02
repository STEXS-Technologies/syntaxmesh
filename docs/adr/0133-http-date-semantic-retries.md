# ADR-0133: HTTP-date semantic retry delays

- Status: accepted
- Date: 2026-09-30

Extend ADR-0132's Shardline-derived retry policy to HTTP-date `Retry-After`
headers using the crates.io `httpdate` 1.0.3 parser, not a custom date parser.
The dependency stays in the CLI host. Compare the parsed timestamp with the
current system clock; elapsed timestamps produce zero delay. Future timestamps
obey the same five-second synchronous limit as integer seconds. Malformed
headers retain fallback backoff. This supersedes ADR-0132's HTTP-date limitation.

Fixed-clock tests must cover past, equal, within-bound, boundary, and excessive
dates. Keep three attempts, offline guards, cache identity, and retryable
Penelope jobs unchanged. Clock skew and durable delayed scheduling remain
outside this policy.

Verified on 2026-09-30: fixed-clock date cases and existing HTTP retry fixtures
passed. `cargo make ci` passed, including dependency auditing, strict Clippy,
architecture checks, migration checks, workspace tests, and documentation builds.

Parser reference: [httpdate](https://docs.rs/httpdate/1.0.3/httpdate/fn.parse_http_date.html).
