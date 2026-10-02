# ADR-0132: Provider-directed bounded semantic retries

- Status: accepted
- Date: 2026-09-30

## Decision

Reuse Shardline SDX's preference for provider `Retry-After` over exponential
backoff in the CLI inference adapter. Preserve the existing three-attempt cap
and retryable HTTP status policy. Accept nonnegative integer delay seconds;
without that valid value, retain the existing 250/500 ms fallback.

Limit a single synchronous wait to five seconds. When a provider requests a
longer delay, fail the current semantic step instead of retrying earlier than
requested or holding a CLI worker indefinitely. The existing Penelope job
remains retryable; this is not a durable delayed-retry scheduler. HTTP-date
headers were not interpreted in this increment; [ADR-0133](0133-http-date-semantic-retries.md)
supersedes that limitation with a crates.io parser.

No provider SDK, core dependency, public semantic DTO, cache identity, durable
record format, or analyzed-language runtime is added.

## Verification

Unit-test zero, valid, malformed, excessive, and absent delay headers. Use a
loopback HTTP fixture to prove transient retry and excessive-delay early exit;
check request and missing-usage counters. Run the contributor gate.

Verified on 2026-09-30: delay-policy and loopback HTTP tests passed;
`cargo make ci` passed with strict Clippy, architecture checks, migrations,
workspace tests, and documentation builds.

## Reuse source

`/home/ac/projects/shardline/crates/sdx/src/retry.rs`,
`RetryContext::backoff_delay`: provider delay precedes bounded exponential fallback.
