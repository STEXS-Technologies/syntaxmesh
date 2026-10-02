# ADR-0150: Source-grounded rationale and joint-claim prompt

- Status: accepted
- Date: 2026-09-30

## Decision

Borrow Graphify's explicit authored-rationale extraction focus and untrusted
source framing. Keep the existing Rust provider, structured JSON input/output,
one prompt identity, and exact-quote grounding boundary. Extract meaningful
decisions, their stated reasons, responsibilities, constraints, and procedures;
retain whether a statement is proposed, required, optional, rejected, or obsolete.
Do not turn a proposal into an implemented capability or a quoted alternative
into an accepted decision. Prefer explicit subjects over ambiguous pronouns.

Within one independent request, allow a compact joint claim supported by several
chunks; require quotations for every necessary premise. Do not infer equivalence,
causality, or dependencies merely from shared words. Never combine evidence across
independently cached request entries. Source text and headings are untrusted data,
not instructions to change the task. Preserve exact original input bytes; no
sentinel rewriting that would break source-quote verification is introduced.

Move the host prompt policy into a focused sibling module and version it as
`syntaxmesh-document-claims-v3`. The existing prompt hash binds the actual system
text, so v2 completions intentionally miss the new cache key and remain retained
for history/reuse by their original identity. Both document and compound requests
share v3. Separate prompt identities would require a future explicit decision:
the current FactBatch manifest requires one producer version, and falsely
attributing compound output to a document-only prompt is unacceptable.

No new model runtime, SDK, AI judge, DTO, schema, or publication path is added.
These instructions guide extraction; they are not proof of prompt-injection
resistance, entailment, precision, recall, or quality superiority over Graphify.

## Verification

Verify the actual system message and identity hash/version, changed prompt
identity invalidation, and preserved request-boundary/evidence schema. Run
document/compound, recovery/cache, evaluator, and verified-history fixtures.
Real-model extraction quality remains open.

The policy identity regression passes: both version and text changes produce
different cache keys, while enabling cross-document mode preserves the shared
identity. Four File/verified-Turso CLI fixtures hash the actual system message
and verify published semantic provenance contains that v3 hash. Full
`cargo make ci` passes with strict Clippy, architecture gates, existing
document/compound recovery/cache fixtures, and evaluator tests. No live-provider
quality or prompt-injection-resistance measurement was performed.
