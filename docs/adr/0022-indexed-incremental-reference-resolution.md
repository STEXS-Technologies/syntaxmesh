# ADR 0022: Resolve only affected reference partitions

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

`Indexer::prepare_delta` currently materializes every node and edge from the
accepted generation on each re-index. It does so to find old references and
resolve them again, despite the plan's requirement to recompute only affected
file/resolution partitions. This adds graph-sized clone and decode work to an
otherwise unchanged or one-file update.

## Decision

Use file ownership, incoming/outgoing adjacency, and exact-name/terminal-name
candidate lookups through the `GraphStore` port. Re-extract changed files only;
revisit existing references only when their source file changed or a matching
definition name was added, removed, or changed. Continue to resolve candidates
through the standalone resolver, retaining explicit ambiguity and missing
references. Turso will maintain an indexed terminal-name column alongside its
canonical payload; existing full-restore/write-staging limitations remain
separate and documented.

## Consequences

- Ordinary incremental preparation no longer clones every node and edge.
- New store lookup methods and a Turso schema migration are required.
- A source-tree scan still compares the file inventory to discover deletions.
- A high-frequency terminal such as `new` can still return many legitimate
  candidates; ambiguity must not be hidden by an arbitrary limit.
- This change does not claim to remove Turso's full in-memory restore or
  full-snapshot transactional write cost.

## Verification

- A counting store rejects whole-graph `nodes()`/`edges()` calls during an
  incremental prepare and records the bounded lookup calls.
- Indexer tests preserve unchanged-scan, one-file-edit, deletion,
  re-resolution, and stale-write behavior.
- Turso migration/reopen tests validate the terminal-name index against node
  payloads, and backend differential tests compare the lookup results.
