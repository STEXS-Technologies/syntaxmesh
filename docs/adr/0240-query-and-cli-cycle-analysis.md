# ADR 0240: Shared Query and CLI cycle analysis

Status: accepted

## Decision

Expose `Query::cyclic_components(relation)` through the existing retained
projection and SCC implementation. Add
`cycles[-turso] <store> <current|generation-id> <calls|imports|all>`.
Return one JSON object with schema version 1, selected generation, relation,
and sorted component arrays of hex node IDs. Validate arguments before opening
stores. Turso retains its existing lease through query/output and rejects active
owners until daemon attachment is implemented.

## Limits

This materializes the complete selected graph and computes cyclic SCCs, not
individual cycle paths or a bounded interactive query. Results inherit accepted
extraction and resolution quality; historical indexed point queries are unchanged.

## Verification

Full `cargo make ci` passes in 212.63 seconds after updating the independent
extension fixture lockfile for the new Query→graph dependency. Strict workspace
Clippy, architecture/dependency checks, extension isolation, and all workspace
tests pass. A real Rust source fixture checks equal File/Turso call-cycle members,
relation filtering, retraction after removing a call, unchanged retained results,
and argument rejection. The ownership fixture now includes `cycles-turso`,
requiring rejection before opening an owned invalid database.
