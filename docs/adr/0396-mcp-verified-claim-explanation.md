# ADR 0396: MCP verified inference-claim explanation

Status: accepted

Date: 2026-10-23

## Context

ADR-0395 provides a bounded Query explanation, but coding harnesses cannot yet
invoke it through SyntaxMesh's native MCP surface. The existing retained-claim
page already returns a versioned claim record; the rule definition remains in
the operator's inference configuration.

## Decision

Add a read-only MCP tool `explain_inference_claim`. It accepts a serialized
versioned `InferenceClaim` copied from an inference page and the corresponding
serialized deterministic `Rule` from the caller's configuration. The MCP host
parses both under an aggregate input-byte cap, queries the claim's exact
generation from its canonical Turso store, and returns the ADR-0395
`InferenceExplanation` only after witness verification succeeds.

The host enforces fixed maximum premise, depth, and output-byte budgets; callers
cannot raise them. The operation does not access or mutate the downstream
inference database and does not infer claims, summarize with a model, or claim
recursive derivation completeness. Existing MCP tools and DTO schemas are
unchanged.

## Consequences

- v4 harnesses can traverse from retained inference-page claims to exact
  generation-pinned proof evidence through one native MCP operation.
- The caller must provide the rule configuration matching the claim's
  rule/version and configuration fingerprint; altered/mismatched witnesses
  fail verification.
- This exposes deterministic direct proofs only. Recursive proof DAGs,
  truth-maintenance state, natural-language `why`, and HTTP/CLI parity remain
  open.

## Verification

MCP tool discovery and direct-handler tests cover the new tool. The
retained-inference fixture copies a real page claim and rule into the tool and
checks exact premise and provenance output, historical generation pinning,
wrong-rule rejection, and MCP input bounds. The Query API test checks its
serialized output-byte limit. Strict MCP Clippy and architecture checks pass.

## References

- SyntaxMesh source of truth v5, sections 89, 104, and 130.
- ADR-0395, verified inference explanations.
