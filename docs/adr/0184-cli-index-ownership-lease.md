# ADR-0184: CLI index ownership lease

Status: Accepted

## Decision

Copy Shardline S3/OCI persistent advisory-file RAII locking into CLI indexing.
Acquire a nonblocking standard-library exclusive lock before opening the File
or Turso store, and retain it through deterministic indexing and optional
semantic publication. Reject contention with a clear usage error rather than
waiting indefinitely. Close the file to release ownership; never unlink it.

Resolve existing database/snapshot symlinks through canonicalization; for new
targets canonicalize the created parent and append the filename. The sidecar is
the resolved target filename with .syntaxmesh-owner.lock appended. Cooperating
index commands therefore share ownership through relative and symlink aliases.
Roots/parents remain trusted and operator-controlled; hard-link aliases,
malicious sidecar replacement, and network-filesystem semantics are not covered.

This is host policy, not a core/store contract. Existing transactional stale-base
guards remain necessary. Only CLI index/index-turso adopt the lease in this slice;
other mutation commands and embedded writers are not yet coordinated. A daemon
must reuse this lease and route all cooperating mutations before claiming single
workspace ownership. No PID file or stale-lock deletion protocol is introduced:
the operating system releases the advisory lock when the owning process exits.

## Verification

Unit tests cover contention, release, and existing symlink aliases. Real CLI
subprocess tests cover rejection before File/Turso store creation and successful
indexing after release. Full `cargo make ci` passed in 130.77 seconds.
