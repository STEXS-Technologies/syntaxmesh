# ADR-0339: Daemon indexed MCP context policy options

Status: implemented for the daemon MCP endpoint.

Expose daemon `--source-content-context` and `--lexical-first-context` for its
MCP endpoint. Both require `--mcp` (which already requires an explicit tokenizer),
reject duplicates, and compose through the existing MCP builders. Defaults and
explicit-seed behavior remain unchanged. HTTP's ordinary context path is not
silently relabeled indexed or changed by these flags. Hot reload is not implied.

Reuse generation-pinned policy-isolated cache behavior and discovery packing
already tested in MCP. This does not promote lexical-first to a default or
claim exhaustive retrieval-quality evidence.

Verification: option tests reject duplicates and missing MCP while preserving
defaults and composition. A real daemon MCP transport fixture with both flags
returns a nonempty generation-pinned context pack and preserves watched search
refresh and HTTP boundary rejection. Targeted process tests and strict all-target
daemon Clippy pass. Full host-policy equivalence remains a separate requirement.
