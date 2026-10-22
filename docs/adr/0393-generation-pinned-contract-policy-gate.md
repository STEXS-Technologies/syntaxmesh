# ADR 0393: Generation-pinned contract policy gate

Status: accepted

Date: 2026-10-23

## Context

Relation contracts can be evaluated directly by the Query API and retained as
durable history, but there is no bounded command suitable for CI or operator
policy checks. Retained history is not policy enforcement: it preserves the
evaluation, while callers must still decide which outcomes are unacceptable.

## Decision

Add an opt-in inference-SQLite CLI command that evaluates a supplied contract
set against one explicitly selected accepted generation and returns a
versioned JSON result. The caller must provide a non-empty `--fail-on` set
containing one or more of `Supported`, `Contradicted`, and
`InsufficientEvidence`. No organization-specific severity or default policy is
implied. The command exits nonzero after emitting its JSON result when any
evaluation matches the caller's failure set.

The command is read-only, does not retain results, and does not schedule
evaluation on publication. File and Turso stores use the same Query evaluator;
the selected generation is validated by that evaluator. Existing contract
configuration admission and work bounds apply, and serialized output remains
bounded by the configuration's `output_bytes` value.

## Consequences

- CI and operators can make explicit, generation-scoped policy decisions from
  evidence-aware outcomes without writing a Rust caller.
- `InsufficientEvidence` remains distinct and is never treated as proof by the
  evaluator; callers may explicitly choose whether it fails their policy.
- Automatic publication-time scheduling, durable policy definitions, and
  organization-specific severity remain out of scope.
- No storage migration or runtime-specific policy behavior is introduced.

## Verification

The CLI integration suite covers passing and failing explicit policies,
invalid/empty policies, output generation pinning, and File/Turso parity.

## References

- ADRs 0376–0384 and 0390.
- SyntaxMesh source of truth v5, section 119.
