# ADR-0223: Shared historical neighbor export formatting

- Status: accepted
- Date: 2026-09-30

## Decision

Expose `HistoricalNeighborPage::into_records` in the runtime-neutral query crate.
Move the existing item/footer conversion into a focused sibling module and make
`Query::historical_neighbor_records` delegate to it. This preserves the existing
temporal export schema, item ordering, provenance, query mode, generation,
direction, returned count, and typed continuation without copying serialization
into host adapters.

This is formatting, not validation or query execution. Callers constructing a
page must validate its contents first. No database, transport, workflow, or host
dependency is introduced. NDJSON remains an explicit export format, not the
HTTP service handoff. Historical CLI daemon attachment remains open: its larger
page limit, typed input cursor, and missing-seed behavior still need adaptation.

## Verification

All 28 query tests pass, including comparison with query-produced records,
round trips through the existing export serializer, empty incoming/outgoing
footers, and nonterminal typed continuations. Strict all-target/all-feature
query Clippy, workspace formatting, and architecture dependency checks pass.
No full-workspace CI run is claimed for this refactoring.
