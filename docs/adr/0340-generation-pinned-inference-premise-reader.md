# ADR-0340: Generation-pinned inference premise reader

Status: implemented for ordered evidence resolution.

Expose an optional Query reader for an inferred claim's exact ordered edge-version
witness. Require the claim's repository/worktree/generation to match the selected
graph and impose a positive premise-count budget. Resolve each premise through
the existing fact/version-change lookup at its valid-from generation, selecting
the exact opened version. Reject missing, foreign, malformed or no-longer-active
premises at the claim selection. Clip later closure bounds at that selection.

Reuse SQLite/Turso's existing indexed start/end lookups, not graph replay or a
whole-history query scan. Reference backend internals may scan history; do not
claim O(1) lookup or uniformly bounded backend work. Preserve witness order and
repeated roles rather than rewriting a rule's evidence into a set.

This resolves evidence, not authentication or proof that an arbitrary supplied
claim follows its named rule. Rule validation, claim identity validation, recursive
proof DAGs and host delivery are separate requirements. Do not imply otherwise.

Verification: shared four-backend lifecycle fixtures resolve exact ordered
premises, clip later closure, and reject foreign scope, closed current witnesses,
and exhausted premise budgets. Targeted Query and SQL process-independent tests
and strict all-target Clippy pass. This is not rule proof verification.

Sequence-binding hardening: resolve opening and closure generation IDs through
the store's authoritative generation-sequence lookup before clipping closure.
A lookup-only fixture rejects substituted opening and closure sequences while
the genuine historical witness verifies. The four-backend lifecycle rerun passes
using a fresh temporary directory; targeted malformed tests and strict Query
Clippy pass. This adds indexed sequence reads, not whole-graph inference replay.
