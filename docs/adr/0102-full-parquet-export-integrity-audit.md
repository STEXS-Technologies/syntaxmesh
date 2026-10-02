# ADR-0102: Explicit full Parquet export integrity audit

- Status: accepted
- Date: 2026-09-29

## Context

ADR-0101 makes fact-history Parquet exports restartable by checking the latest
committed partition when a matching export resumes. Re-reading every earlier
partition on every retry would make resume cost grow with the export, even when
the export is complete and unchanged. A separate audit is needed to establish
that every committed artifact is still present and intact.

Shardline's integrity tooling separates cheap operational checks from explicit
full verification. SyntaxMesh should keep the same distinction: routine resume
validates the recovery boundary; an operator-requested audit can scan the whole
export without coupling the canonical graph store to the analytical files.

## Decision

1. Add an explicit Rust API, `verify_fact_history_export`, to the Parquet
   adapter. It takes the export directory and reads no graph-store backend.
2. Serialize it against a concurrent writer with the export lock. Validate the
   versioned manifest, exact expected artifact file set, every committed
   partition's BLAKE3 sidecar, Parquet readability/schema metadata, and the
   total row count against the manifest. Reject missing, extra, malformed, or
   corrupt artifacts.
3. Return the export identity, committed partition and row counts, and the
   manifest's completion state. A valid incomplete checkpoint can be audited;
   this function does not resume, repair, import, or mutate the export.
4. Keep this explicit O(export-size) audit separate from the bounded recovery
   path. Resume continues to check the latest committed partition and any
   deterministic orphan being adopted; it does not silently turn every retry
   into a full scan.
5. Do not add this operation to graph publication, core/query contracts, or
   service protocols. It is local artifact verification only; NDJSON remains
   explicit user/tool export and is not involved.

## Alternatives considered

- Hash every historical partition on every resume: rejected because routine
  resume becomes proportional to all previously exported bytes.
- Verify only the manifest and latest partition forever: rejected because
  corruption of an older partition could remain undetected indefinitely.
- Repair or regenerate damaged artifacts during verification: rejected; a
  verifier must be read-only and report evidence, leaving recovery policy to
  the caller.
- Verify by reading canonical SQL tables: rejected; the audit is independent
  of live backend availability and validates the published artifact itself.

## Consequences

- Operators can request a complete integrity scan without opening the graph
  store or mutating the export.
- The audit is intentionally linear in exported bytes; it is not part of the
  O(1)-in-history resume path.
- The manifest and per-partition checksums remain the existing artifact
  contract; no additional progress journal or text serialization is added.

## Verification

- Audit a completed export and a valid incomplete checkpoint.
- Detect corruption in an early (not only latest) partition, missing or extra
  files, checksum mismatch, invalid Parquet metadata, and manifest row-count
  mismatch.
- Confirm verification leaves all artifact bytes unchanged and does not need a
  store instance.
