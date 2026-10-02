# ADR-0195: Canonical store paths for index hosts

## Evidence

A cross-process embedded-lease fixture exposed that an existing Turso store
opened through a file symlink could report a missing schema. The lease resolves
the real store, but indexing previously opened the original alias. SQLite-style
WAL sidecars must be associated with the same canonical database pathname.

## Decision

Resolve existing index store targets before opening File/Turso backends. For
absent targets, resolve the parent and preserve the filename. All indexing hosts
use this shared path. Lease acquisition remains before store opening and covers
the complete pass. This assumes operator-trusted paths, not adversarial swaps.

## Verification

Cross-process fixtures hold a public embedded `WriterLease`, reject CLI index
through parent and existing-file aliases before backend opening, and index
successfully after release on both File and Turso. Preserve the raw OS-lock
fixture as independent interoperability evidence. Migration exclusion now also
uses the public host guard.

Verified: three ownership integration tests, twenty host-equivalence tests,
and both watch integration tests pass; strict CLI Clippy, formatting, and
architecture checks pass. Full CI was not rerun after this correction.
