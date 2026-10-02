# ADR-0165: Literal ECMAScript namespace-import members

- Status: accepted
- Date: 2026-09-30

## Context

ADR-0125 records `namespace.member` uses but excludes all computed properties.
Oxc already distinguishes string-literal keys from arbitrary expressions and
decodes escaped literal values. Reuse that parser and the existing exact-name
export/star-barrel resolver instead of introducing another evaluator.

## Decision

Record `namespace['member']` and `namespace?.["member"]` when the immediate
object is an identifier for a namespace import and the key is an Oxc
`StringLiteral`. Retain the decoded key as `imported_name`, the existing
canonical `namespace.member` spelling as `local_name`, and the complete quoted
literal's span as source evidence. Optional access does not prove execution.

Reuse `ImportKind::NamespaceMember`, module resolution, unique exact-name export
binding, ambiguity handling, provenance, and temporal storage unchanged. This
supersedes ADR-0125's blanket computed-property exclusion only for string
literals. Dynamic keys, concatenations, template literals, numeric keys, indirect
namespace aliases, and aliases shadowed by a local declaration remain excluded.
No JavaScript execution, constant folding, or compiler equivalence is claimed.

Bump the ECMAScript extraction semantic version from 8 to 9. Its existing
producer identity and composite-extractor generation fingerprint invalidate
unchanged affected files. The resolver algorithm/version, public DTOs, stored
enum order, dependencies, and database schemas do not change.

Use focused child test modules, following Shardline's sibling-module layout.
Extend existing CLI barrel and Turso restart/history fixtures rather than adding
a parallel resolution or historical-query harness.

## Verification

All 17 ECMAScript tests pass, including four focused literal-member tests that
each exercise TypeScript and JavaScript. They cover quoted and optional keys,
escaped-name decoding, exact source spans, repeated-occurrence identity,
dynamic-key exclusion, and shadowing. The mixed Node/Python CLI monorepo
scenario verifies multi-hop/ambiguous barrel binding and retraction for literal
members. Both Node-profile host tests pass, including retained Turso history
after deletion and restart.

`cargo make ci` completed successfully in 125.07 seconds: formatting,
architecture boundaries, out-of-tree extension, SQLite/Turso migration
registries, all-feature compilation and strict Clippy, dependency checks,
workspace tests, and API documentation. Existing policy-approved dependency
warnings remain; no new exception was added. Producer-version invalidation and
version-sensitive composite-extractor fingerprint tests remain in the shared
indexer/SDK suites. This does not establish representative repository semantic
coverage or close Gate 5.

## Real-corpus upgrade check

On 2026-09-30, an isolated copy of the retained Sim index was upgraded using the
ordinary Rust CLI against the clean, unchanged Sim revision
`63f18995da8c7b3c458546980008db75714ec1c2`. All 2,572 files were accepted;
provenance for 2,548 TypeScript and two JavaScript files changed from extractor
8 to 9. Python/Bash producer versions remained unchanged. Reopened freshness,
logical references, graph-root validation, and File-store integrity passed.
An unchanged repeat reported the same generation as already up to date and
left the completed workflow count at two.

The current graph has 87,486 nodes and 89,767 edges. Its 326 namespace-member
occurrences equal the prior corpus count: this is upgrade/cache compatibility
evidence, not additional literal-member coverage. The 177 added nodes and
edges are source-backed Markdown anchors from the independent documentation
extractor 6-to-7 upgrade (ADR-0141), not ECMAScript growth. Retained old-generation
node/edge/provenance export, with only query-mode fields removed, has the same
SHA-256 as the original index's fact stream:
`8934522f9959dd80058cca627740c02da192baf3f5c01aee524a7abbc4f91d92`.

The original index and Sim worktree were not modified. The isolated copy and
machine-readable observations remain under ignored
`target/validation/sim-namespace-v9-20260930-BgLPOJ/`. No module-resolution
profile or AI inference was exercised by this corpus check; literal binding
coverage remains in the focused extractor and CLI fixtures above.
