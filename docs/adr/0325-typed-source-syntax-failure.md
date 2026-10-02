# ADR-0325: Distinguish source syntax failure before partial ingestion

Status: first contract phase; tolerant publication not implemented.


The shared File/Turso fixture now passes failed mixed-update and repair/restart
coverage: retain the verified prior snapshot after grammar rejection, then repair
both inputs and publish verified history without changing the old snapshot. Its
first run exposed indexer contextualization erasing `SyntaxError` into
`InvalidInput`; contextual filename wrapping now preserves the syntax variant.
Other failure handling stays strict. This is failure atomicity evidence, not
partial per-file publication or durable failed-file coverage.

Strict all-target Indexer/Engine/Turso Clippy passes after replacing a wildcard
error match with exhaustive variant handling and naming the reopened repair store
distinctly; no lint exceptions were added. Full CI remains pending.

Full contributor CI passes in 240.30 seconds, including all 17 backend/restart
scenarios (43.78 seconds), both shared syntax-failure/repair fixtures, strict
workspace checks, independent producer import and generated documentation.
Tolerant ingestion is still unimplemented; this phase verifies the strict
classification and durable safety boundary it must build upon.

Extend the same category to explicit source-error checks in Python/Bash
Tree-sitter and TypeScript/JavaScript Oxc. Parser setup, cancellation and
internal source-range/identity checks stay `InvalidInput`; no broad error
conversion or recovery is introduced. Existing malformed-source tests must
assert the syntax variant, not merely any error. Successful facts are unchanged.

All 12 Python, 24 ECMAScript and two Bash unit tests pass, including explicit
syntax classification for each supported code language. Strict all-target
Clippy passes for these three adapters. Full contributor CI after this extension
passes in 166.29 seconds (exit status 0), including architecture and migration
checks, independent extension conformance, workspace compilation and strict
Clippy, dependency checks, workspace tests and generated API documentation.
Partial ingestion still has no implementation or completion claim.


Add `ExtractorError::SyntaxError(String)` for source grammar rejection. Rust's
Syn parse failure uses this variant; internal identity, offset and extraction
validation retain `InvalidInput`. Default index publication remains atomic and
strict. No errors are ignored and no source is rewritten or executed.

A later explicit partial policy must retain attempted content hash, producer,
generation, bounded diagnostic and failed-file coverage. It must not stamp failed
inputs as successfully processed or silently reuse stale facts as current. Repair
must retry unchanged failed bytes after producer changes. Historical successful
facts must remain queryable. Reuse Penelope publication and StateChronicle
verification, not a second journal. Store/configuration/integrity failures remain
fatal. Typed syntax errors alone do not establish any of those later guarantees.

This is an additive public error variant: exhaustive downstream matches require
updating. Language extractor semantics otherwise remain unchanged; canonical
successful facts and producer versions do not change for this error classification.
