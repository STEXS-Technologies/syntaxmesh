# ADR-0061: Project-local verified-history policy

- Status: accepted
- Date: 2026-09-28

## Context

[ADR-0004](0004-opt-in-generation-verification.md) makes StateChronicle
verification opt-in, and the CLI currently exposes that policy only as a
per-invocation `--verify` flag. The product source document also specifies a
project-level `[history] verified = true` setting and
`syntaxmesh init --verified-history`. Without a project-local policy, users
must remember to repeat the flag on every generation they intend to retain in
the verified chain.

Configuration is a host concern: it must not add filesystem, TOML, or CLI
dependencies to core models, DTOs, runtime protocols, the engine, or workflow
ports. Verification must continue to affect workflow status/receipts only, not
graph facts or generation identity.

## Decision

1. Add optional `syntaxmesh.toml` configuration at the source-root passed to
   `index` or `index-turso`. Parse a `[history]` section with a boolean
   `verified` field; missing file/section/field defaults to `false`. Unknown
   keys are ignored for forward compatibility; a present unreadable or
   malformed config fails indexing with its path and diagnostic.
2. Add `syntaxmesh init [project-root] [--verified-history]`. The root defaults
   to the current directory. It creates a minimal `syntaxmesh.toml` with
   `verified = false`, or `true` when requested. Creation must not overwrite an
   existing path; users with an existing config edit it explicitly.
3. A project setting supplies the default for both CLI indexing commands.
   Per-run `--verify` forces verification on, including when project policy is
   disabled. With no setting and no override, indexing stays `DURABLE` and
   StateChronicle appends no record. There is no `--no-verify` override:
   omitting verification for an intermediate generation breaks the contiguous
   parent chain required to resume verification in that store.
4. Load this policy only in the CLI host immediately before indexing. Embedded
   engine callers keep the explicit `with_statechronicle_verification()` API.
   No setting is added to generation IDs, manifests, or canonical facts.
5. The CLI owns TOML parsing and initialization; use the direct crates.io
   `toml` crate and keep the configuration implementation in a dedicated
   non-root module. Tests cover defaults, malformed config, initialization
   no-overwrite behavior, and CLI/config/flag precedence for File and Turso.

## Alternatives considered

- Keep verification as a per-run flag only: rejected because this leaves the
  documented project policy unimplemented and makes verified history easy to
  interrupt accidentally.
- Put policy in the engine/core or generation manifest: rejected because it
  introduces host configuration into runtime-neutral contracts and makes a
  workflow choice affect deterministic graph identity.
- Use a home-directory global config: rejected because verified-history intent
  belongs to the source project and should be reviewable/reproducible with it.
- Let `init` rewrite an existing config: rejected because initialization must
  not silently destroy unrelated project settings.

## Consequences

- Repeated indexing honors the repository's explicit verified-history policy;
  users keep it enabled for every accepted generation in the verified chain.
- Enabling verification on an existing store still creates an anchor at its
  current generation; prior graph history is not retroactively verified.
- This is local hash-linked integrity/replay history, not signing or a
  portable proof. Signatures, external timestamps, and CI artifact proofs
  remain separate future work.
- A typo in an ignored unknown key does not fail closed; the known
  `history.verified` field remains type-checked and malformed TOML is rejected.

## Verification

- Parse no-config, empty-config, false, and true settings; reject malformed
  TOML and a non-boolean `history.verified` value.
- Prove init creates the requested setting and refuses to overwrite existing
  files.
- Exercise config defaults and the `--verify` override through File and Turso,
  including reopen/status and explicit full-chain audit.
- Run architecture-boundary checks and the full workspace contributor gate.
