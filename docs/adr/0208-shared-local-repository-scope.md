# ADR-0208: Shared local repository scope

Move the existing CLI path-derived repository/worktree identity policy into
`syntaxmesh-source-host`. Hosts supply the same canonical root and reuse the
same derivation bytes; CLI keeps a thin re-export for existing internal callers.
This is local path identity, not Git remote identity or portable clone identity.
Do not substitute Shardline's provider/owner/name scope: that domain would change
existing store and history identities. No new dependency enters Engine or core.

Verify exact previous derivation and distinct-root separation. Canonicalization
remains the host's responsibility, as before. Production daemon wiring remains
open.

Verified: six source-host unit tests and twenty CLI host-equivalence fixtures
pass. Strict source-host/CLI Clippy, formatting, and architecture checks pass.
The preceding ADR-0203–0207 full workspace CI passed in 184.18 seconds; this
scope extraction was verified with the focused checks above, not a new full run.
