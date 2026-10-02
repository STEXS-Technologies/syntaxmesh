# ADR 0261: Durable module input coverage

Status: accepted

Persist the sorted/deduplicated opaque module-input inventory using the existing
durable-record/CAS port. Keys are scoped by repository, worktree, and resolver
fingerprint, with an explicit v1 namespace. Reuse the record port rather than
introducing schema migrations or raw store access on Engine. Empty inventories
are valid. Invalid records fail closed; observations remain host data, not facts.
Public free functions operate on the store port so pre-Engine CLI preparation
can use the same policy as daemon/embedded hosts. They perform no filesystem I/O.

This records coverage only. Hosts must still fingerprint observations before
planning, bind successful coverage updates to source-marker state, and preserve
recovery ordering. It does not by itself enable config-only invalidation.

Verification: File-store reopen retains coverage. Noncanonical seeded records
reject both load and attempted overwrite, preserving original bytes. Scoped
canonicalization and File restart tests plus strict Engine Clippy pass.

Engine exposes scoped load/save coverage methods delegating to the same free
functions; saving reads the currently installed provider's observations. This
lets daemon hosts persist coverage without mutable-store access. Calls do not
acknowledge a graph generation or change source-planning markers.
