# ADR 0234: Lease unattached embedded Turso reads

Status: accepted

## Decision

Every CLI command that opens Turso directly must retain the existing host-only
`WriterLease` from before opening the store through query completion and output.
Reuse the current lease implementation; do not add a second ownership mechanism.
Already attached reads keep their discovery-first, leased-fallback behavior.
Unattached reads reject a cooperating active owner before opening the database;
this safety gate does not implement their eventual daemon attachment.
Mutation hosts already retain their lease and must not acquire it twice.

## Consequences

Standalone output contracts remain unchanged. History, graph, timeline, export,
change-set, and verification reads cannot bypass an active daemon or watch owner.
The coordination remains advisory: other processes must use the same lease.

## Verification

`cargo make ci` passes in 168.52 seconds, including strict all-feature workspace
Clippy, architecture and migration checks, extension isolation, 15 attachment
tests, 20 CLI host-equivalence tests, four ownership tests, ten daemon lifecycle
tests, and 16 backend conformance scenarios. The new ownership fixture invokes
all 19 previously unguarded commands against an owned invalid database and
requires ownership rejection, empty stdout, and unchanged database bytes.
Existing host-equivalence scenarios cover standalone query output after this
change; this does not prove attachment for the newly guarded commands.
