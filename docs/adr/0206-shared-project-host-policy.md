# ADR-0206: Shared project host policy

## Decision

Move CLI `syntaxmesh.toml` configuration into `syntaxmesh-source-host` and
re-export its existing policy types and initialization function. CLI imports
the shared contract. Keep optional-file defaults, unknown-key tolerance,
known-value validation, opt-in StateChronicle history, explicit Node/Python
resolution profiles, and ordered Python roots unchanged.

Project loading and non-overwriting initialization remain host filesystem
operations, not Engine or core responsibilities. No second daemon parser is
introduced. Initialization retains create-new semantics and its existing
partial-write cleanup. This enables consistent daemon configuration but does
not yet create the daemon executable or wire resolver/provider factories.

## Verification

Move existing policy tests into a dedicated child module and retain CLI
initialization, profile/history override, and malformed-policy fixtures.

Verified: all four source-host tests and twenty CLI host-equivalence tests pass,
including non-overwriting initialization and project/CLI verification policy.
Strict source-host/CLI Clippy, formatting, and architecture checks pass. Full
workspace CI was not rerun for this move.
