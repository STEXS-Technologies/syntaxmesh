# ADR-0204: Shared source inventory fingerprint packing

## Decision

Move CLI source inventory packing into the Engine package's runtime-neutral
`source_inventory_fingerprint`. Preserve the existing ordered sequence of
little-endian path lengths, UTF-8 path bytes, and content hashes. Callers supply
deterministically ordered inventory and append resolver/configuration identity
before invoking source planning. This is fingerprint input, not a storage or
transport protocol; it does not use NDJSON or filesystem dependencies.

Use the same packing and durable planner in the watch/live HTTP fixture, removing
its fixture-only in-memory content comparison. Verify repeated reconciliation
does not create new generations after a completed edit. The fixture remains
controlled Rust input, not a production daemon or comprehensive resolver host.

Verified: both Engine planning/packing tests, the native/polling live HTTP
composition test, twenty CLI host-equivalence tests, and both CLI watch tests
pass. The HTTP fixture waits for at least three completed callbacks and requires
the published generation to remain unchanged across reconciliation. Strict
Engine/CLI/HTTP Clippy, formatting, and architecture checks pass. Full workspace
CI was not rerun for this change.
