# ADR-0209: Shared structural source reconciliation

Expose the existing watch fixture's recovery/scan/fingerprint/plan/publication
sequence from the optional source host crate. Reuse Engine planning and Penelope
publication; do not reopen stores or introduce a second indexer. Callers hold
writer ownership and exclusive Engine access, configure its extractor/resolver,
and supply matching extractor fingerprint, extensions, and resolver suffix.

The operation is explicitly structural. It does not execute semantic providers,
reload project configuration, acquire ownership, or claim complete daemon
orchestration. Recovery precedes scanning and no-op decisions. Return selected
generation and whether this pass published. The live watch/HTTP integration uses
this operation rather than its private duplicate sequence.

Verified: seven source-host tests and all eight HTTP tests pass, including
native and polling watch publication, unchanged reconciliation, retained history,
and StateChronicle verification. Strict source-host/HTTP Clippy, formatting, and
architecture checks pass. Full workspace CI has not been rerun for this change.
