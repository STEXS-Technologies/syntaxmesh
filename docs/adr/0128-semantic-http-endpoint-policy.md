# ADR-0128: Apply semantic endpoint policy to actual HTTP requests

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0119 requires explicit authorization for remote inference. The CLI validates
the configured endpoint, but reqwest follows redirects by default and may use
environment-configured proxies. A loopback provider can therefore redirect a
request to a remote URL, or a proxy can receive local inference requests.

## Decision

1. Disable automatic redirects for semantic HTTP requests. Treat 3xx responses
   as provider failures; users configure the intended final endpoint explicitly.
2. Disable automatic proxy discovery for loopback providers. Explicitly enabled
   remote HTTPS providers may continue using environment-configured proxies.
3. Keep these controls in the CLI host's HTTP adapter. Model requests, evidence
   validation, durable retry, and graph publication contracts remain unchanged.

## Verification

A loopback transport fixture returns a redirect to another endpoint. The
provider must return the original HTTP 302 failure after one request rather
than following the redirect. The one-command semantic fixture also configures
unreachable proxy endpoints in the child process's environment and verifies
that its loopback model request succeeds directly. The contributor gate
exercises both tests.

The complete local Linux `cargo make ci` gate passed, including both fixtures,
strict Clippy, and the architecture dependency checks.

## References

- [ADR-0119: One-command semantic indexing](0119-one-command-parallel-semantic-indexing.md)
