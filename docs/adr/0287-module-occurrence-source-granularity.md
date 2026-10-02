# ADR-0287: Classify module occurrences consistently during evidence packing

Status: accepted; compiler and backend verification complete; retrieval quality unproven.

Reuse the existing canonical NodeKind and SourceGranularity classifier. Import,
Export and ModuleResolutionDiagnostic facts describe source-backed module
occurrences/outcomes, not symbol or documentation definitions. Their old Local
classification contradicted that canonical meaning and the retrieval-family
classification used in ADR-0284.

Classify these three kinds as Occurrence alongside resolved/unresolved/ambiguous
references. Keep source evidence, graph navigation and retained facts intact.
Do not filter them from traversal, alter producer versions or storage, introduce
another ontology, change item-kind precedence, or reorder relevance/distance.
Only otherwise tied evidence can be affected by this granularity tie break.

Require explicit Import/Export/diagnostic classifier fixtures and strict Query
tests/Clippy, followed by full contributor verification before quality claims.
This consistency correction does not solve cross-family allocation or establish
improved retrieval quality.

Explicit classifier fixtures cover a named import, local export and unresolved
module-resolution diagnostic alongside the existing reference variants. All 47
Query tests and strict all-target Query Clippy pass. Full contributor verification
is running. Existing context backend fixtures check ordinary/ranked compiler
retention, but do not contain these three occurrence kinds; their passing alone
must not be described as end-to-end occurrence tie-break coverage.

Full contributor CI subsequently passes in 174.68 seconds, including all 17
backend scenarios, contributor boundary/migration checks, strict workspace Clippy
and documentation. Occurrence-specific end-to-end packing coverage remains open;
no retrieval-quality improvement is claimed from this consistency correction.

Occurrence-specific end-to-end coverage now extends the existing temporal backend
fixture: synthetic Import/Export/unresolved-diagnostic facts clone canonical
source/provenance, with IDs deliberately preceding the definition's ID. An
ordinary explicit plan gives all four equal relevance/distance and verifies the
definition's source item is first while every occurrence item remains present.
Current and historical packs match across InMemory/File/SQLite/Turso, then remain
equal after source edits and durable reopen. The scenario and strict Turso
all-target Clippy pass. This is typed packing coverage, not parser-extraction or
real-repository relevance evidence. Full CI has not been rerun for this fixture.

Subsequent full contributor CI completes successfully in 127.05 seconds, including
all 17 backend scenarios and contributor gates. This closes verification for the
expanded typed packing fixture. Cross-family allocation and representative
repository retrieval remain separate open work; no default policy is promoted.
