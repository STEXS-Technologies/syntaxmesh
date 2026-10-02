# ADR 0236: Expose retained fact-history pages through Query

Status: accepted

## Decision

Add `Query::fact_history_page(after, limit)` returning the existing store
`FactHistoryPage`. The query's generation pins the scan and its cursor; reject
zero limits and limits above 1000 before delegating to the store port.
Reuse SQLite/Turso composite fact-history indexes and their existing conformance
coverage. Do not replay deltas or materialize complete history in the query layer.

## Limits

This scans globally ordered fact versions, not a single identity's history.
It is a bounded query primitive, not HTTP/CLI attachment or a constant-time
whole-history query. Reference backends may use their existing scanning default.

## Verification

All 33 query tests and strict all-feature workspace Clippy pass. The dedicated
fixture compares pages with the store delegate, checks retained pinning after a
later removal, and rejects invalid limits, foreign-generation cursors, and missing
generations. Full CI including the subsequent HTTP exposure passes in 213.04 seconds.
