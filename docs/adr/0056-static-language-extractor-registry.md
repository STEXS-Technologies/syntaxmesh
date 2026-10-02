# ADR-0056: Compose language extractors through a static registry

- Status: accepted
- Date: 2026-09-28

## Context

The embeddable engine and indexer are generic over one `LanguageExtractor`.
This keeps parsing host-independent, but a real repository containing more
than one language cannot currently be routed through one engine instance. The
v0 plan requires TypeScript/JavaScript and Python language packs, and its
extension architecture expects file-extension metadata at the language-pack
boundary. Adding parsers before adding composition would leave mixed-language
repositories unsupported or force each host to write its own router.

Shardline has no language parser/registry subsystem to copy. The useful
existing SyntaxMesh pattern is the generic engine boundary: composition can
be implemented in `syntaxmesh-language-sdk` without changing the engine,
indexer, graph model, store, or runtime ports.

## Decision

- Add an SDK-owned, host-independent `CompositeExtractor` which maps normalized
  file extensions to existing `LanguageExtractor` implementations.
- Hosts register concrete extractors and their extensions during construction.
  Registration rejects empty or duplicate extensions; routing is
  case-insensitive and uses the normalized source path's final extension.
- Preserve the existing `LanguageExtractor` trait and single-extractor API.
  `CompositeExtractor` implements that trait, so it is usable anywhere the
  current engine/indexer accepts an extractor.
- Keep the registry immutable during extraction and local to an engine
  instance. It loads no dynamic libraries, reads no host configuration, and
  creates no thread/runtime requirements.
- An unregistered or extensionless source returns the existing typed
  `UnsupportedFile` error. Each selected pack continues to own its own
  provenance and extraction semantics.

## Alternatives considered

- Add a central language switch to the CLI, engine, or core: rejected because
  it couples product composition to one host and makes each new language
  require edits in central orchestration code.
- Add extension discovery directly to `LanguageExtractor`: rejected for now;
  that changes the contract for every existing/custom extractor and mixes pack
  metadata into the minimal one-file extraction interface.
- Use runtime plugin loading or an out-of-process protocol: deferred until its
  independent isolation/conformance gate; it is not required for the current
  in-process Rust/TypeScript/Python v0.1 packs.

## Consequences

- Mixed-language repositories can use one runtime-neutral engine by supplying
  a composite extractor; no store or graph contract changes are needed.
- Language packs must declare their supported extensions at composition time.
  Overlapping extensions are configuration errors rather than precedence rules.
- Registry unit tests must cover normalization, routing, duplicate/empty
  registration, and unsupported paths. Each actual language pack still needs
  its own semantic, determinism, reference, and corpus fixtures.
- This decision does not claim multiple language packs are implemented; it
  establishes the composition boundary needed to add them cleanly.
