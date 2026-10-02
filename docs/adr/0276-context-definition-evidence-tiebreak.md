# ADR 0276: Definition evidence before tied reference occurrences

Status: accepted, 2026-10-01

Whole-Sim bundle `context-20261001T200845.921074Z-uncommitted` admits
49 source items for the identifier-seeded TypeScript case while omitting
15 source items and every signature/path/summary. Numerous admitted items
are single-line reference occurrences in the same file as the omitted,
selected function definition. Stable-ID order is not a usefulness signal.

Within the existing source-evidence priority, retain relevance and graph
distance ordering, then prefer non-reference node evidence over reference,
unresolved-reference and ambiguous-reference occurrences. Stable keys remain
the final deterministic tie breaker. Explicit seeds and closer graph nodes
retain their existing priority. Reuse typed node kinds rather than inferring
definitions from source text or introducing another search service.

Preserve complete source lines, exact token counting, omission accounting,
current/historical compiler sharing and existing public DTOs. This does not
fix absent candidates, guarantee definition inclusion or establish whole-corpus
quality.

Verification: all 38 Query tests, strict Query Clippy, the three current/retained
context backend/restart scenarios and full contributor CI pass (200.55 seconds;
all 16 backend-conformance scenarios). Whole-Sim bundle
`context-20261001T201306.336049Z-uncommitted` restores the selected TypeScript
declaration's source item in the same eight-seed diagnostic, reducing admitted
source items from 49 to 18. The restored evidence is declaration line 897, not
the complete function body. Natural-language acceptance remains 3/10.
Independent whole-Shardline bundle `context-20261001T201640.085185Z-uncommitted`
preserves its baseline 6/10 natural-language target/budget hits, including both
documentation targets; Python metadata remains absent. These are narrow target
regressions, not answer-body coverage or release-quality proof. Shardline and CI
ran concurrently, so their timings are not isolated performance evidence.
