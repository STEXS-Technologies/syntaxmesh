# ADR-0174: Host-authorized extension manifest

- Status: accepted
- Date: 2026-09-30

## Decision

Add `FrameCodec::read_batch_for` for deployment hosts that have independently
authenticated a producer and selected its authorized manifest. Before reading,
validate the caller-supplied manifest. After ordinary bounded decode and SDK
validation, require exact namespace and producer-version matches, and require
every declared capability to be in the caller's grant. Capability order is not
significant and a producer may declare fewer capabilities than its grant.
Reject mismatches before returning any batch to Engine ingestion. Discard the
stream on failure, as for all framing errors.

Adapt Shardline's verify-context-then-required-scope host boundary
(`shardline-server/src/auth.rs`), without copying JWT, HTTP, or service dependencies.
Use existing SDK capability enums, not a parallel role/permission system.
Ordinary `read_batch` remains a codec-only API and is explicitly not authorization.
No wire version or existing SDK contract changes.

The caller must not derive the grant from the received batch, nor treat this
comparison as peer authentication. This adapter provides no credentials,
socket ownership check, remote attestation, or trust promotion. Runtime origins
remain producer assertions and the Engine still classifies their trust.
Production peer authentication, lifecycle, deadlines, receipts, and independent
process-failure conformance remain separate open host work.

## Verification

Nine codec tests pass, including rejection of otherwise valid namespace/version
changes and capability escalation, invalid grant rejection before any stream
consumption, order-independent/subset capability checks, and clean EOF. Seven
Engine extension-ingestion tests pass: the Unix stream fixture uses the grant
check before publication, and an unauthorized but internally valid producer
is rejected without state or workflow changes before a valid producer succeeds.
Strict Clippy for all targets of both adapter and Engine, architecture boundaries,
and formatting pass. These checks prove grant enforcement, not peer authentication.
