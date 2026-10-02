# ADR-0217: Bounded active-owner discovery read

- Status: accepted
- Date: 2026-09-30

## Decision

Expose host-only `read_active_discovery` for an existing store. Reuse the canonical
sidecar naming and ownership probe. Return `None` when no cooperating writer is
observed, ignoring stale discovery contents. While ownership is active, require
a regular non-symlink discovery file and read at most 64 KiB plus one rejection
byte. Missing, non-regular, or oversized records are errors, not absent-owner
fallback. Recheck ownership after reading; disappearance is an error.

Reuse Rust bounded `Read::take` rather than trusting file metadata length or
reading unbounded bytes. Do not open the database, create files, or alter metadata.
Use the same operator-controlled-directory assumption as the lease/publisher.

These observations are not an atomic ownership transaction: an owner may exit
and a new owner may acquire between them. Bytes are untrusted until the caller
validates schema, canonical store/scope, loopback endpoint, and owner-instance
identity through every attached request. Neither `Some` nor `None` authorizes
direct database use; fallback must retain the real lease. This does not yet
wire automatic daemon attachment.

## Verification

Fifteen ownership-host tests pass. Reader fixtures cover absent/inactive metadata,
active owner without a record, exact 64 KiB round trip, oversized active record
rejection, ignoring an oversized stale record after release, canonical aliases,
and symlink rejection. Strict ownership Clippy and architecture checks pass.
This does not prove an atomic owner transition or endpoint authentication.
