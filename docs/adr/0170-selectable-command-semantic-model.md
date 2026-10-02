# ADR-0170: Selectable command semantic model

- Status: accepted
- Date: 2026-09-30

## Decision

Keep local harness transport generic. An argv element exactly equal to `{model}`
is replaced with the selected `--semantic` model before command validation,
execution, and configuration hashing. No shell interpolation or provider-specific
argument rewriting is performed. Literal argv elements remain unchanged.

For command transport, bare `--semantic` defaults to `gpt-6-luna`; explicit model
names override it. Existing HTTP transport retains its Ollama default. The supplied
Codex configuration uses the model placeholder and medium reasoning. Users may
edit the configuration to select other reasoning settings or harnesses.

The effective model and resolved argv both participate in existing cache identity.
Changing a model must not reuse another model's completed extraction.

## Verification

CLI unit tests cover defaults, explicit overrides in either option order, resolved
argv, and configuration-hash separation. The File/verified-Turso subprocess
fixture checks the actual model argument, a new call after switching models, and
cache reuse when returning to the original model. It also verifies that argv-file
and inline forms share cache identity and all retained snapshots survive switches.
`cargo make ci` passes on the combined implementation (109.27 seconds).
