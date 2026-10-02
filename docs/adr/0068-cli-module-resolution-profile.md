# ADR-0068: Opt in to the CLI Node module-resolution profile

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0067 makes module resolution an injected capability because resolving source
specifiers requires filesystem and project-configuration access. The embedded
engine must remain runtime-neutral, and silently choosing a resolver profile
would make graph edges depend on host assumptions. The CLI already uses the
project-root `syntaxmesh.toml` pattern for opt-in StateChronicle verification.
Shardline similarly composes TOML configuration at the host boundary rather
than pushing it into its storage/domain crates.

Oxc's resolver exposes separate `import` and `require` package conditions.
SyntaxMesh retains the source occurrence kind, so the selected Node profile can
choose these conditions per occurrence without guessing from the repository as
a whole.

## Decision

1. Add the optional project-local setting:

   ```toml
   [module_resolution]
   profile = "node"
   ```

   Absent setting means resolution is disabled. `node` is the only accepted
   profile in this increment; invalid profile values are configuration errors.
2. The CLI owns filesystem access and constructs the Oxc adapter for the
   canonical source root. The core, resolver contract, Indexer, and Engine do
   not read `syntaxmesh.toml` or the operating system filesystem. Embedded
   Engine callers continue to inject their own provider explicitly.
3. The Node profile uses Oxc with project `tsconfig` auto-discovery and the
   source extensions supported by the SyntaxMesh ECMAScript pack. Static and
   dynamic imports and source re-exports select the `node` + `import`
   conditions; `require()` selects `node` + `require`. Additional user-defined
   package conditions are not enabled by this profile.
4. Include the selected resolver identity/settings fingerprint in the CLI's
   deterministic generation identity. Enabling or changing resolution must
   therefore publish a distinct generation even when source bytes and
   extractor versions are unchanged.
5. StateChronicle policy remains independent: enabling the Node profile does
   not turn on verified-history recording.

## Alternatives considered

- **Resolve by default in every CLI project:** rejected; this introduces
  filesystem/config-dependent graph facts without explicit project intent.
- **One repository-wide `import` or `require` mode:** rejected; mixed ESM and
  CommonJS source already carries the syntax distinction needed to choose
  conditions per occurrence.
- **Read project config from Engine/Indexer:** rejected because these are
  runtime-neutral application components and embedded hosts may supply a
  virtual filesystem or another resolver implementation.
- **Silently infer a Bundler/NodeNext profile from package files:** rejected;
  profile selection is explicit and future profiles require their own tested
  semantics and ADR update.

## Consequences

- CLI users opt in through one project-local setting; existing projects retain
  current unresolved-import behavior until they select a profile.
- The CLI composes Oxc and its OS filesystem without adding filesystem
  dependencies to core or the engine.
- ESM/CommonJS package export conditions follow the actual import occurrence.
- A resolver configuration change changes the next generation ID, making
  resolution updates visible in ordinary graph history.
- This profile does not add symbol/member binding, package ingestion, or
  resolution diagnostics.

## Verification required

- Config parsing accepts `node`, defaults to disabled, and rejects unknown
  profile names while preserving the documented policy for unrelated future
  TOML keys.
- CLI fixtures prove opt-in indexing resolves only indexed modules, chooses
  ESM versus CommonJS package conditions correctly, and retains unresolved
  external targets without fabricated nodes.
- Changing the profile changes the generation fingerprint; StateChronicle
  verification remains independently opt-in.
- Architecture checks prove CLI-only filesystem composition; Engine and
  runtime-neutral contracts remain host-independent.

## Implementation status

Implemented on 2026-09-28. `syntaxmesh init --node-module-resolution` writes
the opt-in setting without overwriting an existing config. The CLI constructs
Oxc with `FileSystemOs` only for the configured canonical source root and
includes the provider identity/settings fingerprint in its generation ID.
`ModuleResolutionRequest` carries `Import` versus `CommonJs`; Oxc shares its
filesystem cache while selecting per-occurrence conditions. Tests cover
profile parsing/defaults, CLI indexing and edge retraction after disabling the
profile, unchanged `DURABLE` verification status, and conditional package
exports for both import and require modes. Durable resolution outcome
diagnostics and the read-only CLI query are implemented by
[ADR-0070](0070-versioned-module-resolution-diagnostics.md); non-Node profiles
remain open under ADR-0067.
