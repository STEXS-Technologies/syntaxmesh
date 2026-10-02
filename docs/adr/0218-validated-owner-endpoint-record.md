# ADR-0218: Validated owner endpoint record

- Status: accepted
- Date: 2026-09-30

## Decision

Add host-only `OwnerEndpoint`: schema version 1, canonical existing store path,
literal loopback `SocketAddr` with nonzero port, and a fresh 256-bit OS-random
instance identifier encoded as lowercase hexadecimal. Reuse Shardline's
`getrandom::fill` pattern and locked crates.io dependencies; never use PID/time
as instance identity or silently fall back on random-source failure.

Encode one bounded JSON object for the existing atomic discovery publisher.
Reject unknown/duplicate fields, unsupported versions, foreign canonical paths,
non-loopback/zero-port addresses, malformed identifiers, and oversized input.
Only publish through the matching store's retained lease. Discovery combines the
active-owner bounded reader and strict record validation; absent ownership returns
no endpoint, while invalid active-owner metadata remains an error.

This identifier detects accidental stale/replacement endpoint use only when the
future transport checks it for every attached request. It is not a bearer secret,
remote authentication, or proof that an advertised listener owns the store.
Do not enable attachment until the daemon instance check is wired and tested.
No core/public graph DTO/protocol crate gains host dependencies. JSON is bounded
host discovery metadata, not NDJSON graph storage or a graph service handoff.

## Verification

Seventeen ownership-host tests pass, including record round trip, fresh distinct
identities, IPv4/IPv6 loopback construction, matching-lease publication, active
discovery and stale-record rejection. Invalid schema, store, addresses, identity,
unknown/missing fields, and oversized input are rejected. Strict ownership
Clippy and architecture checks pass. Eight daemon and three CLI ownership
process tests remain green. No live transport identity-check claim is made.
