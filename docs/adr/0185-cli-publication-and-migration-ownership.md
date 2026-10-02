# ADR-0185: CLI publication and migration ownership

Status: Accepted

## Decision

Reuse the persistent advisory lease from ADR-0184 for extension publication and
explicit SQLite/Turso schema migration. Hold it through publication or migration
and the resulting receipt/status output. This coordinates these commands with
indexing on the same resolved store target without introducing another lock.

Extension grant/frame validation remains before lease acquisition and store
opening. Invalid input therefore cannot create a store or an ownership sidecar.
Valid input fails fast on contention before opening an Engine or durable workflow.

This remains cooperating CLI host policy, not mandatory embedded-store locking.
Daemon lifecycle, other mutation paths, hard-link aliases and hostile filesystem
replacement remain outside this guarantee. Transactional stale-base checks are
still required; readers do not acquire this exclusive writer lease.

## Verification

Real subprocess tests reject both migration commands before database creation
under contention and migrate successfully after release. Existing extension
publication tests now hold the indexing sidecar, reject imports without a
receipt, release ownership, and verify successful publication for File and Turso
with verification enabled and disabled. Both integration suites pass (22 tests).
Strict CLI Clippy passes for all targets/features.
Full `cargo make ci` passed in 122.89 seconds, including architecture boundaries,
the independent producer fixture, migration registries, and backend conformance.
