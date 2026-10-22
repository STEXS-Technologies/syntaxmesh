# ADR 0379: Index contract evidence once per evaluation

Status: accepted

Date: 2026-10-22

## Context

The bounded contract evaluator compared every contract with every retained
premise. Its work therefore grew as `contracts × active_edges`, despite the
query needing only one exact `(source, target, relation)` lookup per contract.
This repeated work made otherwise small contract batches expensive on large
retained selections.

## Decision

Validate the selected premises once and build a query-local index keyed by exact
source, target, and serialized relation. Retain the lowest `EdgeId` for each
key so evidence selection remains deterministic. Resolve each contract through
that index, preserving existing outcomes, identities, and exact fact-version
citations.

`ContractLimits::edge_visits` bounds premise entries examined while building
the index, rather than multiplying that work by contract count. Contract
validation, encoded-input, result-count, and history-scan bounds remain
independent. The index is ephemeral and adds no persistence or public DTO.

## Consequences

- Evaluation work is bounded by one pass over selected premises plus indexed
  contract lookups, rather than a nested contract/premise scan.
- Query results are byte-for-byte semantically equivalent and remain
  generation-pinned.
- The Query adapter still loads its retained selection for each request. This
  decision does not implement durable contract-result history or v5 Stage C.

## Verification

Test all outcomes and deterministic evidence selection, prove a batch with
many contracts consumes premise-visit budget only once, and retain independent
budget failure coverage. Run inference tests, strict Clippy, and formatting.

## References

- SyntaxMesh source of truth v5, sections 316 and 320.
- ADRs 0376–0378.
