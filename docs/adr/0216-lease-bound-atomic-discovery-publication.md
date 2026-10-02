# ADR-0216: Lease-bound atomic discovery publication

- Status: accepted
- Date: 2026-09-30

## Decision

Let `WriterLease::publish_discovery` publish up to 64 KiB of host metadata to
the canonical store's `.syntaxmesh-owner.discovery` sidecar. Keep the lease's
lock file separate and never replace or unlink its inode. Retain the canonical
store path in the guard; publication is available only through the held guard.

Copy Shardline index local filesystem publication's sequence: create-new
temporary file in the same directory, write, sync the file, atomically rename,
then sync the directory on Unix. Reuse the already locked `tempfile` crate for
temporary-file lifetime and cleanup instead of custom temporary naming. Reject
non-regular existing discovery paths, including symlinks. Assume operator-owned
directories, matching the existing lease's trust boundary. Unix temporary files
are owner-only; do not claim equivalent Windows ACL guarantees.

Metadata is not graph storage or an internal NDJSON handoff. This low-level
publication primitive does not interpret bytes, authenticate an endpoint, or
enable CLI attachment. A future discovery reader must reject oversized/invalid
records, require active ownership, and bind endpoint requests to the advertised
owner instance. A crash may leave a complete stale record; absence of active
ownership makes it unusable. Never infer a live owner from the record alone.

Successful rename followed by directory-sync failure may already have replaced
the record; return the error and do not claim rollback. The canonical graph is
never opened or modified by this operation.

## Verification

Thirteen ownership-host tests pass. Publication tests cover initial/replacement
bytes, 64 KiB boundary, retained exclusive lock and unchanged lock/store bytes,
temporary-file cleanup, owner-only Unix permissions, stale-record non-ownership,
and directory/symlink rejection. Strict ownership Clippy and architecture checks
pass. Eight daemon lifecycle and three CLI ownership process tests also pass;
workspace formatting passes. The full CI result for ADR-0215 predates this
publication primitive.
