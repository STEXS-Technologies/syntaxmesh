# ADR-0211: Initial shared-owner daemon host

Add `syntaxmeshd` as a thin executable composition of existing writer ownership,
source-host setup/reconciliation, native/polling watch, and shared-Engine HTTP.
Require an explicitly migrated Turso database outside the source root and a
literal loopback bind address. Load project policy once at startup; configuration
changes require restart. No migration occurs implicitly.

Reuse CLI's ctrlc termination handler and HTTP's retained Engine/runtime drop
ordering. Native registration and its initial reconciliation finish before
reporting readiness. A watch failure shuts HTTP down and returns failure. Stop
signals finish an admitted publication, stop HTTP gracefully, join the writer,
then drop the HTTP runtime and retained Engine before releasing ownership.
There is no bounded shutdown deadline. Reads serialize with publication through
the existing Engine mutex; this is not backend concurrent-read support.

This is the first executable structural daemon slice, not completion of source
document section 13 or Phase 6. Local IPC, automatic CLI attachment, MCP serving,
git monitoring, semantic command scheduling, analytics scheduling, configuration
reload, and crash/socket lifecycle tests remain required follow-up work. Do not
reimplement graph or workflow semantics in this executable.

Verified: process fixtures cover native/polling saves, current no-op stability,
retained historical reads after graceful restart, competing daemon exclusion,
SIGTERM shutdown on Unix, and invalid/missing/inside-root/unmigrated database
rejection without overwriting database bytes. Signal delivery reuses the CLI
tests' safe `nix` API, not an external source-runtime process. Full workspace CI
passes on ADR-0208–0211 (168.28 seconds), including strict Clippy, architecture,
extension import, migration registries, backend conformance, and documentation.
Existing dependency-policy warnings remain; live-model quality evaluation is
still opt-in and is not established by this run.
