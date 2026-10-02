# Project navigation and architecture rules

Read `README.md` and `docs/V0_ARCHITECTURE_PLAN.md` for current scope. The two source-of-truth documents under `docs/` define product intent. Record new architectural decisions as ADRs before changing public contracts.

Keep core, public DTO, and thin runtime protocol crates free of database, workflow, transport, and host dependencies. Use Penelope for durable full-engine workflows; add StateChronicle generation-verification support from the first implementation phase with runtime verification opt-in. The Change Engine is a separate project and owns mutation workflows.

Keep `lib.rs` and `mod.rs` files limited to module declarations and public re-exports; put implementation logic in focused sibling modules. Keep substantial unit-test suites in dedicated `tests.rs` child modules rather than inline in large adapters.
