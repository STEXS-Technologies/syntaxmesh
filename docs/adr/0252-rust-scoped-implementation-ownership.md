# ADR 0252: Rust scoped implementation ownership

Status: accepted

## Decision

Reuse existing containing-declaration paths and evidence-span node lookup in
the impl visitor. Include function, impl-method, and default trait-method paths
in local impl header identities, not just inline module names. Maintain a source
owner stack so impl containment points to the immediate emitted module/function/
trait/impl context instead of always to the file module. An unmodeled enclosing
declaration has an unknown owner; do not invent a containment edge across it.

Register an impl header before entering its own type path, preserving top-level
and module-level header identities. Local impl ordinals no longer depend on
same-header impls in differently named containing declarations. Duplicate blocks
within one identical declaration path still use existing occurrence ordinals.
This does not solve lexical block identity or compiler trait dispatch.

Advance Rust producer to 0.12.0. Historical facts remain immutable; normal
producer invalidation replaces current extraction. No new DTO, dependency,
storage format, or runtime is added.

## Verification

All 27 Rust extractor tests and extractor strict Clippy pass. New fixtures check
unrelated local impl insertion preserves the existing scoped ID, actual module/
function/impl-method/trait-default containment, and no invented containment from
unmodeled constant and associated-type owners. Existing body/offset stability and polarity tests
remain intact. Shared File/verified-Turso tests add an unrelated same-header impl
while changing a call, then verify scoped impl/containment IDs, repeat caching,
restart, and retained historical facts. All 14 shared lifecycle tests pass.
Full `cargo make ci` passes in 221.63 seconds, including strict workspace Clippy,
architecture and migration checks, extension isolation, all 16 backend-conformance
scenarios, and API documentation. Existing allowed dependency advisories remain
unchanged. This verifies source identity/ownership, not compiler-equivalent
binding or dispatch.
