# ADR-0307: Separate authoritative history transfer from schema validation

Status: implemented; focused checks pass, representative capture pending.

ADR-0306 attributes 26.403471 seconds of a whole-Sim cold build to schema
reads, but that region includes SQL, payload transfer, hashing and decoding.
Shardline's immutable content-addressed cache does not directly apply to the
mutable database rows here: existing tests require changed authoritative bytes
to invalidate a warm schema cache. A generation-only cache would violate that
contract. Do not substitute cache trust for validation.

Extend the existing private, opt-in Turso benchmark instrumentation to emit
successful schema-read payload bytes, SQL/transfer elapsed time and hash/decode
elapsed time. Reuse the existing profile environment switch; introduce no new
public API, database schema, dependency or production logging. Normal builds
must not acquire clocks. Use this breakdown before choosing a compact-storage
or caching change; these wall-clock regions do not establish physical I/O costs.

All 46 instrumented Turso unit tests pass, including the warm-cache history
corruption regression. Strict all-target/all-feature Turso Clippy, formatting
and the no-default-feature Turso check pass. Whole-Sim capture
`context-20261002T113824.153336Z-uncommitted` is running with the previous
repository scope and disk-backed scratch policy. Full contributor CI and
representative transfer/validation attribution remain pending; no optimization
or speedup is claimed.

Full contributor CI subsequently passes in 194.28 seconds with two build jobs,
including all 17 backend conformance scenarios (45.58 seconds), all-feature
workspace tests, mixed instrumentation checks, strict Clippy, migration and
extension gates, dependency policy and documentation. Existing allowed
dependency-policy warnings remain. Representative capture is still live;
successful CI does not establish an optimization or performance acceptance.

Whole-Sim capture subsequently completes successfully in 519.12 seconds, with
publication at 378.934 seconds and all five required 8,192-token targets retained
(three of five at 2,048 tokens). The index contains the same 87,486 nodes,
567,068 postings and 21,071,384 charged bytes across 88 pages. Build time is
36.901669 seconds, including 35.839577 seconds in page reads and 1.061821
seconds processing. Cold context is 37.411926 seconds; retained 8,192-token
requests are 0.742644–0.994825 seconds in this single debug sample.

The 88 schema reads before the index-build marker each transfer a 33,580,768-byte
history payload: 2,955,107,584 cumulative bytes, with 22.143901 seconds in
SQL/transfer and 1.911296 seconds hashing/decoding. The first hash/decode region
includes 1.256956 seconds for the initially uncached entry. Repeated full-record
transfer, not repeated deserialization or term processing, is the dominant
measured schema cost. Explore an authoritative compact-read/invalidation pattern
before increasing caches. Any proposal must account for direct SQL mutation,
missing rows, generation changes, reopen/restart and corruption regression;
do not infer safe cache reuse from immutability of application-level generations.
Matching counts and one timing sample do not establish a speedup or an SLA.
