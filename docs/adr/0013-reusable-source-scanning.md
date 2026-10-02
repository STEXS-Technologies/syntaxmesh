# ADR 0013: Reusable source scanning boundary

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

The source roadmap calls for a filesystem scanner with ignore/Git-ignore
support, normalized paths, and BLAKE3 content identities. The current CLI owns
all of that behavior, forcing embedded hosts to reimplement source discovery
before they can call the same engine API. That weakens host equivalence and
makes scanner correctness and traversal cost difficult to test independently.

## Decision

Add a host- and language-neutral `syntaxmesh-scanner` crate. It accepts a root
and an explicit list of file extensions, applies `ignore` traversal rules,
normalizes relative paths to slash-separated strings, reads text inputs, and
computes deterministic file IDs, content hashes, and sizes. It returns sorted
`SourceFile` values plus traversal metrics. Language choice remains with the
host; parsing, resolution, and publication remain in the engine pipeline.
Project-local ignore rules are respected, while machine-specific global Git
ignore rules are disabled so the same checkout does not scan differently on
different hosts.

The CLI will call this scanner for `.rs` inputs rather than maintain a private
walker. Bounded parallel scanning, binary/generated-file policy, and
changed/new/deleted scan manifests remain subsequent work; this initial API
must not imply those capabilities.

## Consequences

- CLI and embedded hosts can share source discovery without introducing host
  dependencies into core, language contracts, or the engine.
- Scanner policy becomes independently testable and benchmarkable.
- Files with selected extensions are currently read as UTF-8 text; binary
  source policies and configurable ignore semantics remain future contracts.
- The workspace gains one focused source-scanning crate, implementing an
  already documented roadmap boundary rather than adding scanner policy to the
  CLI or core model.

## Alternatives considered

- Keep scanning in the CLI: rejected because embedded hosts would duplicate
  filesystem semantics and the scanner could not be exercised independently.
- Put scanning in `syntaxmesh-engine`: rejected because hosts may supply
  already acquired source from non-filesystem systems, and filesystem policy
  does not belong in the application engine.
- Add scanning to `syntaxmesh-core`: rejected because filesystem and ignore
  dependencies violate the pure-domain boundary.

## Migration and compatibility

The CLI keeps `.rs` inclusion, project-local ignore behavior, normalized paths,
file identity, and generation fingerprint inputs; machine-specific global
ignores no longer alter its inputs. No persisted schema or existing wire
contract changes. Embedded hosts may adopt the scanner independently while
continuing to pass their own `SourceFile` values.
