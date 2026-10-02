# ADR-0270: Reference ranked candidates

Status: accepted, 2026-10-01.

Expose a generation-pinned reference candidate reader for index validation.
Reuse normalized document frequencies, historical node pages, integer logarithmic
rarity and multi-term coverage (the useful Graphify principles). Keep the best
1–256 candidates with deterministic NodeId ties. Reject incomplete statistics;
never substitute capped frequencies. The caller supplies an explicit node scan
budget. A second paged pass scores nodes with the complete pinned statistics.

This is deliberately a reference reader, not a new persistent text engine or the
default context path. It scans the selected generation twice, retaining at most
one node page plus the top candidate set. Durable indexed implementations must
match it before adoption. Canonical substring search remains unchanged.
The Query wrapper exposes the same reference reader pinned to its generation,
so existing host evaluation closures can exercise it without receiving a store.

## Whole-corpus evidence

Whole-Sim run `context-20261001T133737.880105Z-uncommitted` completed the
reference comparison: TypeScript target rank 46, Python 222, JavaScript 1,
Bash 182, Quickstart 3. This does not qualify exact normalized-token ranking
as a production policy. Python exceeds the 64-candidate budget and Bash regresses
from the substring probe's rank 3. Morphology and boundary matching require
investigation before a persistent index is built around this score.
Production context remains unchanged and its strict retrieval evaluation fails.

A detached debugger sample during the reference scan found graph-delta decoding;
inspection confirms Turso historical_nodes_matching decodes the full generation
history entry on every page merely to obtain manifest.schema_version. This is
avoidable repeated work to investigate separately, not a measured bottleneck
attribution or authorization to bypass historical schema validation.
