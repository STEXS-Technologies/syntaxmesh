# ADR-0267: Shared scanned-source reconciliation

Status: accepted, 2026-10-01.

Expose `reconcile_scanned_sources` in the host crate, accepting the existing
immutable SourceFile inventory and policy fingerprints. The directory-scanning
reconciler delegates to it. CLI indexing uses this same resolver-input validation
and publication implementation while retaining its scan metrics and explicit
semantic enrichment phase. No extra scan, dependency or workflow is introduced.
Recovery occurs before planning; callers retain exclusive writer ownership.
