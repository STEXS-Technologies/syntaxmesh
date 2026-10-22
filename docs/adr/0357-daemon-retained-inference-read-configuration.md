# ADR-0357: Daemon Retained-Inference Read Configuration

Status: accepted

Date: 2026-10-22

## Context

ADR-0356 adds opt-in retained checkpoint and bounded-claim reads to HTTP and MCP.
The APIs accept operator-installed `InferenceReadBindings`, but the shared-owner
daemon currently has no configuration path to install them. Therefore the
feature cannot be enabled by an ordinary daemon operator. These reads must stay
separate from canonical graph storage and must not introduce migration or
workflow effects during host startup.

## Decision

Add a daemon-only opt-in `--inference-read-config <path>` option behind the
`inference-sqlite` feature. The file is strict, versioned JSON containing
operator-selected page ceilings and a bounded list of named bindings. Each
binding contains a downstream database path and a path to the complete
`StreamOrigin` receipt emitted by the existing CLI setup command. Relative
database and origin paths resolve against the configuration file's parent; the
resolved downstream path must be outside the indexed source root and must not
alias the canonical graph database. The configuration is capped at 1 MiB and
each origin receipt at 16 KiB; the registry retains its existing 256-binding
and identifier constraints.

The daemon derives repository/worktree scope from its configured source root,
constructs one registry, and installs cloned bindings into HTTP and, when
enabled, MCP. It performs no migration, recovery, or write to the downstream
database. Database migration and inference synchronization remain explicit CLI
operations. No remote request can select a database path or alter bindings.

Without the feature, the daemon option is rejected with a clear build-feature
message. With the feature, omitting the option leaves retained inference reads
disabled.

## Consequences

Operators gain one shared read configuration for both daemon hosts without
coupling core, DTO, or runtime-protocol crates to filesystem or daemon concerns.
The configuration is an operational contract that must be versioned and
documented. Relative paths improve relocatability but are anchored to the
operator-selected config file, not to the repository. Reusing a CLI-generated
receipt avoids a second public receipt encoding. The daemon will fail
startup on malformed, out-of-scope, duplicate, oversized, or canonical-store
aliasing configuration, but does not probe or create the configured downstream
database; explicit migration remains authoritative.

## Verification

Parser and loader tests cover valid versioned configuration, relative
database/origin path resolution, duplicate and foreign scope rejection, unknown
fields, size limits, missing downstream-file non-creation, and feature-off
rejection. A feature-enabled daemon lifecycle test verifies malformed config is
rejected before canonical reconciliation. A positive feature-enabled lifecycle
test verifies one configured binding reaches both daemon HTTP and MCP, serves
the same retained checkpoint, and leaves downstream database bytes unchanged;
HTTP also serves its retained page. Separate HTTP and MCP host tests cover
two-claim pagination and cursor continuation.

## References

- [ADR-0356](0356-host-bound-retained-inference-reads.md)
- [V0 architecture and implementation plan](../V0_ARCHITECTURE_PLAN.md)
- [Daemon options](../../crates/syntaxmesh-daemon/src/options.rs)
- [Daemon host composition](../../crates/syntaxmesh-daemon/src/host.rs)
