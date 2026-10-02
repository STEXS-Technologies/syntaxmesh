# ADR-0164: Immutable context wire fixtures

- Status: accepted
- Date: 2026-09-30

Borrow Penelope's `penelope-domain/tests/golden_wire.rs` pattern: check in literal
JSON fixtures and compare decoding with independently constructed typed values,
then compare serialization with the immutable JSON value. Runtime-created
round trips alone can let serializer and deserializer drift together.

Cover `ContextRequest`, the current schema-v2 `ContextPack`, and legacy schema-v1
pack decoding with unknown evidence classes. Preserve fixed identities, field
names, enum spellings, nulls, omission counts, warning fields, and version tags.
Do not derive expected values from the fixture itself or regenerate fixtures
automatically on failures. JSON whitespace and object-key order are not pinned.

Fixtures are test assets, not graph storage or service handoffs. No DTO changes,
schema bump, dependencies, runtime launch, or lint exceptions are introduced.
They verify wire compatibility, not tokenizer correctness or source entailment;
the query/compiler fixtures remain responsible for those checks. This closes a
specific context compatibility gap, not the full public-contract release gate.

## Verification

All 19 API-model tests pass, including the three new literal fixtures and the
existing eight-class classification round trip. Strict crate Clippy, workspace
formatting, and the architecture dependency check pass. No production DTO code
was changed; source/token budgeting remains covered by existing compiler tests.
