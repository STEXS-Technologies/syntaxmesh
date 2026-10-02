# ADR-0196: Canonical Turso backend paths

## Decision

Canonicalize local database paths at the Turso adapter boundary, extending
ADR-0195 beyond indexing to read hosts, extension hosts, migration, and status.
Existing paths resolve to the real file. New migration targets use a canonical
parent and original filename. This reuses the standard-library path pattern
already used by the Shardline-derived host lease, without adding a dependency.

Turso's WAL sidecars must use the same database pathname across aliases.
The adapter does not acquire a writer lease: ownership remains a host concern.
Missing open targets still fail without creation; migration remains explicit.
Migration does not create missing parent directories. Dangling symlinks are
rejected during path resolution rather than creating a divergent database.
Paths are operator-trusted; hostile replacement races and hard-link aliases
remain outside this guarantee.

## Verification

Add adapter fixtures that migrate through a parent alias, then reopen through
an existing file alias, inspect migration status, and retain durable records.

Verified: all 39 Turso adapter unit tests and 23 CLI host-equivalence/ownership
integration tests pass. Strict all-feature Turso Clippy, formatting, and
architecture checks pass. Follow-up full `cargo make ci` passed in 170.86
seconds, including all-feature workspace tests and documentation. Existing
allowed dependency warnings remain for `bincode`, `paste`, and `cfg_block`.
