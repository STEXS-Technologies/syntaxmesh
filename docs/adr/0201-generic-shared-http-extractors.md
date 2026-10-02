# ADR-0201: Generic shared HTTP extractor hosting

## Decision

Parameterize `SyntaxMeshHttp` by a Send-capable language extractor, defaulting
to `RustExtractor` for source compatibility. Keep standalone `open` specialized
to that default. Shared-engine construction and read routes are generic over
`LanguageExtractor + Send + 'static`. Cloning a host clones shared ownership,
not its extractor, so no Clone bound is imposed on extractors.

Use SDK composition from ADR-0200 for mixed-language shared engines; do not add
HTTP-specific extraction dispatch. Core/SDK contracts remain runtime-neutral.
The live host keeps the same single Engine lock, request snapshots, admission,
and shutdown semantics. Reads serialize with publication. IPC and watch wiring
are separate, unfinished daemon responsibilities.

## Verification

Upgrade the live TCP fixture to a Send composite Rust/documentation Engine,
publish edits, and compare current/retained graph results without restarting.

Verified: all seven HTTP tests pass. The live fixture registers Rust, TS, JS,
Python, Bash, Markdown, and text; searches require positive current/historical
symbol and heading evidence, not merely equal empty results. Strict HTTP Clippy,
all-feature workspace compilation, formatting, and architecture checks pass.
Full workspace CI was not rerun for this generic host change.
