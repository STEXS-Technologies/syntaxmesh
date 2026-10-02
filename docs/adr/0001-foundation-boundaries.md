# ADR 0001: Engine, runtime, and reliability boundaries

Status: accepted for v0 planning

Date: 2026-09-27

## Context

SyntaxMesh must run as an embeddable Rust engine and as a standalone service, accept public extensions, and remain local-first. Its source document requires Penelope for durable full-engine workflows and a StateChronicle integration surface from day one. Thin probes and core types must remain lightweight.

## Decision

Keep domain, API DTO, and runtime protocol crates independent of Turso, DuckDB, Penelope, StateChronicle, daemon, and transport dependencies. Compose the full engine with a required Penelope workflow adapter for multistep durable work. Add a StateChronicle generation-verification adapter in the first implementation phase; verified-history recording is opt-in. All hosts call one engine application API. An extension uses public manifest, namespace, provenance, and fact contracts.

## Consequences

More crate boundaries and conformance fixtures are needed early. They keep runtime probes small, allow host relocation, and prevent future ontology, temporal, or provider work from requiring a rewrite of core ownership. Penelope and StateChronicle adapters need version pinning and compile probes before production code assumes their APIs.

## Verification

Dependency check; embedded/standalone equivalence; stale-generation recovery; verified-history on/off equivalence; out-of-tree extension fixture; thin probe dependency audit.

## References

- [SyntaxMesh §§1, 42.4, 66.4, 70, 72, 104](../syntaxmesh-todo-extensible-opensource-v5.md)
- [Change Engine §§0–3, 5, 92](../syntaxmesh-change-engine-source-of-truth.md)
