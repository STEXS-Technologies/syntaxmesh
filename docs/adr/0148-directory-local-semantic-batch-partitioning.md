# ADR-0148: Directory-local semantic batch partitioning

- Status: accepted
- Date: 2026-09-30

## Decision

Borrow Graphify's same-directory grouping for generation-wide semantic inputs.
Partition by normalized parent directory, normalized path, then source span and
node ID within each file, rather than hashed file/node IDs. Existing byte and
chunk bounds still control packing; a batch may cross a directory boundary.
Paths select batch membership inside the runtime-neutral engine and are not
added to provider messages. Request canonicalization remains unchanged.

This supersedes ADR-0118's file/node-ID partition order. Independently cached
document extraction is unchanged. Compound cache membership may change, including
after a rename; existing content-addressed records remain valid for their inputs.
No schema migration, model call, new dependency, or scheduler is introduced.
Directory proximity is a heuristic, not topic selection or corpus-wide reasoning.
No speed or quality superiority is claimed without a real-model benchmark.

## Verification

Use deliberately interleaved file identities and reversed node identities to
verify directory grouping, source-order partitioning, repeated deterministic
cache keys, and unchanged chunk bounds. Run the existing semantic/cache and
verified-history fixtures through the full CI gate.

The locality regression passes with two directories, four documents, and eight
chunks. Full `cargo make ci` passes, including strict Clippy, architecture gates,
and File/verified-Turso cross-document, cache, recovery, and history fixtures.
Live-provider quality evaluation remains opt-in and was not run.
