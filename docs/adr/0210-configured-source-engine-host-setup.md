# ADR-0210: Configured source Engine host setup

Compose existing source-host policies into a reusable setup function for an
already-owned store and canonical root. Reuse bundled Send extractors, local
scope derivation, explicit project resolvers, and Engine verification opt-in.
Return the configured Engine with its matching extractor and resolver planning
fingerprints. Project verification or an explicit positive override enables
verification; the override does not disable project policy.

This copies the existing CLI setup sequence, not graph semantics. Store opening,
migration, writer lease, configuration loading/reload, semantic execution, and
daemon lifecycle remain caller responsibilities. The real native/polling
watch/HTTP fixture exercises this setup with the full supported source pack.

Verified: eight source-host tests and eight HTTP tests pass. Policy tests cover
default-unverified, explicit opt-in, and project-required verification; native
and polling watch fixtures retain current/no-op/historical publication checks.
Strict source-host/HTTP Clippy, architecture, and formatting checks pass.
