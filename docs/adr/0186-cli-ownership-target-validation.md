# ADR-0186: Reject ambiguous ownership targets

Status: Accepted

## Decision

Before acquiring the existing CLI writer lease, reject a store target whose
canonical path is not a regular file. If canonicalization reports NotFound,
reject any existing directory entry at the supplied target (including a dangling
symlink) rather than assigning a new-filename lease. Propagate metadata errors
other than NotFound. A genuinely absent target remains supported.

This prevents a dangling store symlink from assigning a different lease from its
eventual destination, and prevents creating sidecars for directory targets.
Existing regular-file symlinks and canonical-parent aliases remain supported.
This does not add protection against concurrent hostile filesystem replacement,
hard-link aliases, or network filesystems; parents remain operator-controlled.

## Verification

Six ownership unit tests cover contention/release, existing target symlinks,
new-target parent symlinks, directory targets, directory sidecars, dangling
targets, and symlink sidecars with an unchanged destination. Both CLI integration
suites pass (22 tests), and all-target/all-feature strict CLI Clippy passes.
