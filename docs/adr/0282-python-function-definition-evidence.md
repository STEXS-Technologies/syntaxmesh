# ADR 0282: Python function-definition evidence from the existing AST

Status: accepted, 2026-10-02

Reuse the tree-sitter definition-range pattern already used by the Bash adapter
and ADRs 0280–0281. Python Function nodes cover their function-definition AST
node, including signature/body; class-name and call/import occurrence ranges
remain unchanged. Decorators outside that node are not invented as part of its
range. Ownership already uses emitted IDs rather than identifier-range lookups.
Keep qualified IDs and ordinals stable; bump semantic producer version 3 to 4
to re-extract unchanged inputs. No parser, dependency, schema or runtime change.

Move the existing substantial suite into the sibling `extractor/tests.rs`,
following the existing Rust/ECMAScript test layout. Verify exact original-source
slices, multiline/async/nested ownership, Unicode and shifted-line identities.
This changes evidence extent, not natural-language candidate selection. Large
definitions remain subject to complete-evidence exact-token-budget admission.

All 12 Python tests and strict all-target/all-feature Clippy pass. The new
multiline fixture checks async/nested/method source slices, call ownership,
Unicode and stable IDs after leading-line movement. The preserved import and
resolver regressions remain green. Full CI, producer-upgrade retained history
and representative retrieval after this Python change remain unverified.

Selected-file Sim `context-20261001T221020.276019Z-uncommitted` passes the
existing required-target gate. Python's complete definition fits at 8,192
tokens (and explicit seeding fits at 8,190), but is omitted at 2,048 where its
former identifier line fit. This is a real low-budget tradeoff of whole-evidence
admission, not an improvement in candidate selection; body-sized evidence
partitioning remains a separate design problem. No whole-repository score after
Python version 4 is claimed.

The source-host integration suite reuses ADR-0281's File-store upgrade pattern
for Python 3→4, TypeScript 10→11 and JavaScript 10→11. Synthetic prior identifier
evidence is indexed through Engine/Penelope, unchanged input is re-extracted by
the real current extractor, and reopening preserves both exact source ranges,
symbol IDs/call edges and the audited StateChronicle chain. All three fixtures
and strict source-host Clippy pass. This is reference File-store coverage, not
execution of archived producer binaries or SQLite/Turso upgrade coverage.

Full contributor CI after Python version 4 and the three source-host upgrade
fixtures passes in 146.18 seconds, including all 16 backend-conformance
scenarios, workspace/doc tests, strict Clippy, architecture/migration/extension
checks, dependency policy and API documentation. This gate predates the next
test-only all-fact channel experiment; it is not retrieval-quality evidence.
