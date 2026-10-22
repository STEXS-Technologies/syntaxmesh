# ADR 0384: CLI-authored generation-scoped contract sets

Status: accepted

Date: 2026-10-22

## Context

Contract evaluation and retained history are available through the Engine API,
and retained results can be advanced over later graph generations. CLI users
still cannot author a contract set without writing a Rust caller. Catch-up
depends on knowing which complete declaration set is authoritative. Competing
sets at one generation are ambiguous and must not silently become the future
catch-up source.

## Decision

Add a versioned JSON input and File/Turso CLI command that accepts a complete,
non-empty contract set for one existing canonical generation, evaluates it
through the Engine, and durably retains each exact result through Penelope.
The file uses human-readable hexadecimal node IDs, explicit origins,
expectations, tagged relation kinds, and independent contract/input/visit,
history-scan, and output-byte bounds. The command returns the retained receipt
and exact generation-scoped evaluations.

A later generation may publish a replacement complete set: contracts omitted
from that set are no longer the catch-up baseline, while their historical
results remain readable. Repeating the same complete set at the same generation
is idempotent. A different set at the same generation, an empty set, or a set
older than the current authoring head is rejected before new Penelope effects.
Engine callers receive the same guard; this prevents ambiguous latest sets from
being created through the public Engine method. No update-in-place or inferred
deprecation semantics are added: changing a contract requires an explicit
version and a later accepted graph generation.

The public Engine request is runtime neutral and carries only the selected
generation, declarations, and independent bounds; Penelope request construction
remains inside the Engine adapter.

This command records and evaluates evidence; it does not enforce policy, fail
CI on outcomes, or automatically run when canonical graph publication occurs.

## Consequences

- CLI operators can author the first retained set, replace the active baseline
  at a later generation, and then use bounded catch-up and as-of reads.
- Penelope journals the complete input before materialization; exact retries
  remain idempotent and failures recover through the existing workflow path.
- Historical sets/results remain append-only. Explicit lifecycle statuses,
  retirement reasons, policy enforcement, and publication-time scheduling
  remain future work.
- No inference schema migration is needed.

## Verification

Versioned config validation and ID/relation parsing pass. File/Turso CLI tests
cover exact retries, conflict rejection, later-generation replacement,
backward-generation rejection, catch-up from the replacement set, and retained
historical results. Engine recovery and File/Turso reopen tests pass. The
focused config, Engine, and inference CLI suites passed, and `cargo make ci`
passed in 161.44 seconds on 2026-10-22, including formatting, architecture and
migration registries, workspace checks, strict Clippy, dependency/security
gates, workspace tests, doc tests, and API docs. The routine CI target excludes
the opt-in DuckDB adapter.

## References

- ADRs 0376–0383.
- SyntaxMesh source of truth v5, sections 82, 269, and 316.
