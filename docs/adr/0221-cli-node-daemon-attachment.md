# ADR-0221: CLI node daemon attachment

- Status: accepted
- Date: 2026-09-30

## Decision

Extend existing guarded attachment to `node-turso` and `node-at-turso`, reusing
the HTTP node route and typed Node schema. Share the bounded HTTP envelope reader,
proxy/redirect policy, timeouts, and instance-header checks with search. Validate
the returned node ID and, for explicit historical selection, exact generation ID.
Do not output a successful response for a different node or generation.

Reuse one CLI node formatter for embedded and attached current/historical reads;
preserve tab-separated bytes and the historical generation prefix. Active-owner
errors never fall back. No-owner embedded operation retains the writer lease
through Engine lifetime and output. HTTP missing-node/generation errors remain
explicit attachment errors rather than hidden embedded retries. Other read and
mutation commands are not covered by this decision.

## Verification

The real CLI/shared-owner process fixture compares current and explicit historical
node output byte-for-byte with embedded baselines, then publishes edited sources:
the old generation remains readable while the removed node fails current lookup.
All four attachment fixtures, twenty host-equivalence fixtures, strict CLI
Clippy, and architecture checks pass. Existing adversarial search fixtures still
cover the shared transport's identity, redirect, proxy, byte-limit, and schema
guards. Dedicated transport fixtures now return valid typed nodes under the
correct instance header while substituting the current node, historical node,
or historical generation. Real CLI processes reject all three with no stdout
and unchanged fake database bytes. Full `cargo make ci` passes for node attachment
and substitution fixtures (151.65 seconds), including all-feature workspace
tests, strict Clippy, migration registries, extension isolation, dependency
policy, and API documentation. Existing allowed dependency warnings remain.

Additional process assertions reject unknown active-owner generations with HTTP
404 and no output, then verify retained-generation byte equivalence and removed
current-node failure through leased embedded fallback after owner release.
These assertions were added while CI was running and pass separately with all
four attachment fixtures and strict CLI Clippy.
