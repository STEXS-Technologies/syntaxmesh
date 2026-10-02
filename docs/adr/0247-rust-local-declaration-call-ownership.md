# ADR 0247: Rust local declaration call ownership

Status: accepted

## Decision

Reuse the existing declaration and reference collectors for items nested inside
function, impl-method, and default trait-method bodies. A shallow `syn::Visit`
pass finds local items without descending their bodies; the collectors handle
those bodies recursively with enclosing declaration-qualified names.
`CallVisitor` stops at item boundaries so their calls are not attributed to an
outer function. Evidence-span lookup retains distinct duplicate declaration
owners. Advance the Rust producer to 0.9.0 for cache invalidation.

No new DTO, parser, runtime, workflow, or storage format is introduced.

## Limits

This is declaration/call ownership, not lexical target binding or compiler scope
analysis. Block identities, closures as separate callable nodes, local constants'
initializer calls, macro expansion, and declaration identity redesign remain
open. Closure expressions keep existing containing-function ownership.
Historical generations remain unchanged.

## Verification

All 20 Rust extractor tests pass. New fixtures check multi-level local-function
ownership and local declarations inside impl/trait bodies, including loss of
outer impl receiver context. Shared File/verified-Turso CLI fixtures cover
no-op repetition, nested-function body edits, retraction, and retained history.
Full CI is verified by the subsequent ADR-0248 run: `cargo make ci` passes in
215.88 seconds, including workspace strict Clippy and all 16 backend-conformance
scenarios.
