# ADR-0366: Registered Inference CLI Configuration

Status: accepted

Date: 2026-10-22

## Context

The Engine and Penelope layers now support a complete registered inference
baseline/setup/successor path, but the CLI configuration only admits unregistered
version-1 rule sets. The user-facing workflow therefore cannot supply the
ontology definitions required by registered evaluation.

## Decision

1. Preserve the existing version-1 CLI configuration and its unregistered
   setup/sync behavior unchanged.
2. Add version 2 with explicit bounded ontology definitions and independent
   registry-entry, registry-byte, and rule-binding budgets. Reject unknown
   fields, unsupported versions, and non-positive bounds.
3. For version 2, configuration preflight binds the rules to the supplied
   ontology registry and reports the resulting semantic identity without
   opening either store.
4. Keep admission bounds in the pure inference crate; Penelope re-exports the
   existing type path for source compatibility.
5. Existing `inference-setup[-turso]` and `inference-sync[-turso]` commands
   dispatch to the registered Engine operations for version 2. The same
   versioned configuration is journaled by Penelope for restart recovery.
6. Keep migrations explicit and retain existing lock/ownership checks. Do not
   alter version-1 files or request payloads.

## Consequences

Registered inference becomes usable from the existing CLI workflow without a
second command family or hidden ontology defaults. The CLI remains a thin host;
schema validation, generation scope, durable effects, and recovery stay in the
inference/Engine/Penelope layers.

## Verification

Version-1 compatibility, strict version-2 admission, config-only semantic
identity, and File/Turso registered setup and sync pass in the inference-enabled
CLI suite. Strict all-target CLI Clippy passes. Restart recovery continues to
reuse the versioned registered request journal exercised by ADR-0365; this ADR
adds dispatch/configuration coverage rather than a separate interruption test.

## References

- [ADR-0355](0355-explicit-cli-inference-workflows.md)
- [ADR-0361](0361-ontology-fingerprints-in-inference-identity.md)
- [ADR-0365](0365-registered-inference-stream-setup.md)
