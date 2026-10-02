# ADR-0193: Reusable host writer lease

## Decision

Move the existing Shardline-derived standard-library file lease from the CLI
into `syntaxmesh-ownership-host`. Expose `WriterLease::acquire` and a typed
`OwnershipError`; keep CLI error presentation and existing writer call sites.
Core, DTO, and protocol crates do not depend on this host crate.

The guard owns the locked file handle until dropped. The persistent sidecar
must never be deleted to bypass ownership. Canonical store paths and canonical
parents for absent stores preserve existing alias behavior. Acquiring ownership
does not create the store, but may create its parent and sidecar.

## Scope and limits

This enables future daemon and embedded host reuse; it does not automatically
coordinate arbitrary store users. Cooperating writers must hold the guard
through their complete mutation lifetime. Existing regular-file and dangling
symlink checks remain. Operator-trusted paths are assumed; hostile filesystem
races, hard-link aliases, and network-filesystem locking are not guaranteed.

## Verification

Move the existing six lease fixtures with the implementation; retain CLI
cross-process writer exclusion tests and strict lint/architecture checks.

Verified: six host-crate unit tests, twenty CLI host-equivalence tests, two
cross-process ownership tests, and two native/polling/signal watch tests pass.
Targeted strict Clippy, formatting, and the architecture boundary gate pass.
Follow-up full `cargo make ci` passed in 154.61 seconds, including all-feature
workspace tests and documentation. Existing allowed dependency warnings remain:
unmaintained `bincode` and `paste`, and missing `cfg_block` license metadata.
