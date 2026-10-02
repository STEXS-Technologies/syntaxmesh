# ADR-0172: Reusable semantic concepts and complete evidence

- Status: accepted
- Date: 2026-09-30

## Decision

Use the live evaluator's retained quality findings to revise the existing shared
extraction instructions, not the public graph schema or claim validator. Reuse
Graphify's separation of named concepts from rationale, adapted to SyntaxMesh's
existing source-supported triples rather than importing Graphify node attributes,
confidence rules, or a second inference pipeline.

Prompt v4 asks for reusable component/concept labels, consistently authored names,
separate responsibility/rationale relationships, concise conventional relation
verbs, and quotes containing the necessary premise as well as consequence.
It explicitly states the validator's existing non-empty/distinct subject-object
and non-empty evidence requirements. Proposed/optional/rejected status and
independently cached request boundaries remain unchanged.

The prompt version and exact text hash invalidate old semantic cache entries;
old accepted generations remain queryable. HTTP and command transport use the
same instructions. No deterministic alias merge, relation rewrite, automatic
entailment verifier, provider API, runtime, or storage contract is added.

Compare a new live evaluation with the retained v3 controlled fixture, recording
improvements and remaining drift without inferring representative precision or
Graphify superiority from one sample.

## Verification

Policy tests bind v4 and its exact text hash, distinguish v3 cache keys, and check
that HTTP and command identities share the same policy. Document-only and joint
File/verified-Turso scenarios pass, including actual system-prompt provenance.
Combined `cargo make ci` passes (110.17 seconds).

The v4 live evaluator run under `target/semantic-provider-results/run-niaQzU`
used the same model/configuration and original/edited source hashes as v3's
`run-8lekPn`. It accepted seven initial and eight edited claims; all cache/history
checks passed. Penelope became an independently reusable concept, Change Engine
subject spelling stayed consistent within this sample, and rationale quotes
contained full premise/consequence sentences. Relation wording still drifted
(`exclude`/`excludes`), StateChronicle remained a compound capability label, and
one `engine motivated_by runtime agnosticism` claim needs outcome-versus-motivation
review. The retained `quality-review.json` records these limitations. This is one
stochastic controlled comparison, not a general quality or speed guarantee.

On a copy of the retained v3 store, v4 offline indexing reported two cache misses,
zero provider calls, and no accepted-generation change. The old historical fact
export remained SHA-256
`777e165430d0998f22b51ef7222de66b0ee39cc10b45cb5191b29e3448a10916`
before and after the check; logical integrity, root, and references stayed valid.
No old cache was mislabeled as new-policy inference and no historical facts were
rewritten. Offline failure is expected here, not a successful enrichment run.
