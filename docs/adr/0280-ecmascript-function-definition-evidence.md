# ADR 0280: Parser-native ECMAScript function-definition evidence

Status: accepted, 2026-10-02


Named function declarations and class methods cover their complete AST
definition. Named arrows cover their binding through the end of the arrow
expression. Call, import, binding and reference occurrences retain exact narrow
ranges. Class evidence remains unchanged in this slice. Stable symbol identity
and owner relationships remain unchanged. Bump the ECMAScript semantic producer
version so unchanged inputs are re-extracted; retained generations keep their
original evidence. No public schema, dependency, runtime or storage change.

The existing exact tokenizer and whole-evidence admission policy remains: a
large definition may be omitted rather than cut into misleading partial text.
This fixes evidence extent, not candidate relevance or whole-repository quality.
Verify original UTF-8 slices, adjacent-definition exclusion, arrows, methods,
call ownership and identity continuity across leading-line shifts.

Verification: all 24 ECMAScript tests and strict all-target/all-feature Clippy
pass. The new multiline fixture checks original UTF-8 source slices for named
functions, two neighboring methods and a named arrow, plus stable IDs after a
leading-line shift. Existing ownership/import/reference regressions remain green.
Selected-file Sim evaluation `context-20261001T212100.939723Z-uncommitted`
passes: all five target-seed diagnostics return source evidence at 8,192 tokens,
including TypeScript; the natural-language TypeScript target also fits at 2,048
and 8,192. This five-file fixture does not establish whole-repository relevance.
The preceding whole-Sim run `context-20261001T211650.069911Z-uncommitted` failed
during indexing with a temporary database quota error and produced no scores.
Full contributor CI now passes (215.08 seconds), including all 16 backend
conformance scenarios, workspace/doc tests, architecture/migration checks,
strict Clippy and API docs. It ran concurrently with whole-Sim evaluation, so
the timing is not isolated performance evidence. Dedicated producer-upgrade
retained-history coverage remains open.

Whole-Sim `context-20261001T215901.259609Z-uncommitted` now finishes publication
and evaluation. Explicit TypeScript target seeding admits its expanded function
evidence at 8,183 tokens within the 8,192 cap. Default natural-language retrieval
still misses that target at both budgets; total production hits remain 3/10.
This verifies source admission, not improved default selection.

Dedicated source-host TypeScript/JavaScript upgrade fixtures now pass using the
existing verified File-store Engine pattern. They retain synthetic version-10
identifier evidence, re-extract unchanged input with real version 11, and audit
stable IDs/call edges, exact old/new ranges and StateChronicle history after
restart. These do not execute archived version-10 binaries or establish durable
SQLite/Turso producer-upgrade coverage.
