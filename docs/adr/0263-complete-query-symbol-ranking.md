# ADR 0263: Complete-query symbol ranking

Status: rejected experiment; prior ranking restored

Remove the full-query-size bonus for any one-word symbol merely contained in a
multiword natural-language query. It promotes generic names such as `Page` above
symbols matching several query terms. Retain the exact-symbol bonus only when
the entire normalized query has one term. Multi-term coverage and the existing
complete-function-match preference remain unchanged. No synonym list, fixture
name, model, or corpus-specific weighting is introduced.

The whole-Sim regression exposed this policy error, but fixes must be checked
against other corpora and tight budgets. This does not address substring lookup
candidate truncation, term frequency, or broader semantic relevance. It changes
ranking, not canonical facts, graph IDs, or historical storage.

Baseline: complete whole-Sim release evaluation over 2,572 files retrieved 3/10
strict targets across 2,048/8,192 budgets; TypeScript, Python, and Bash missed at
the required larger budget. Evidence bundle `context-20261001T124000.686476Z-uncommitted`.
Targeted ranking regression and strict query Clippy pass. Whole-corpus follow-up
completed in `context-20261001T124150.489162Z-uncommitted`: strict retrieval fell
from 3/10 to 1/10, losing Quickstart at both budgets without recovering the three
existing required misses. Reject the production scoring change and restore the
prior policy. All 33 query tests pass but do not establish relevance. The added
test characterizes the existing generic-symbol bonus. Investigate candidate
lookup/truncation before another scoring experiment.
