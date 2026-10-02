# ADR-0205: Reusable supported source pack

## Decision

Move CLI extractor registration into optional `syntaxmesh-source-host` as
`supported_source_extractors`. Use the existing SDK Send composition router and
unchanged Rust, Python, TypeScript/JavaScript, Bash, and documentation extension
mappings. Preserve producer/cache identity; no source-language runtime is
launched. CLI and shared HTTP fixtures reuse the same pack rather than defining
independent production dispatch.

The Engine and public SDK do not depend on this bundled pack. Embedded hosts
may still construct custom or local non-Send registries. Scanner inventory,
resolver policies, semantic providers, workflows, and storage are not owned by
the pack. This is reusable daemon preparation, not the daemon executable.

Verified: the pack identity/thread-transfer fixture, twenty CLI equivalence
tests, both watch integration tests, and all eight HTTP tests pass. Strict
pack/CLI/HTTP Clippy, formatting, and architecture checks pass. Full workspace
CI was not rerun after this extraction.
