# ADR 0271: Content-keyed history schema decode cache

Status: accepted, 2026-10-01

Historical Turso node pages currently deserialize the complete generation history
entry on each page only to obtain its schema version. Whole-corpus reference
ranking repeats those pages; a debugger sample and source inspection identified
this avoidable work. This decision does not claim it explains all evaluation cost.

Reuse Shardline's bounded content/revision-keyed cache principle, not its async
service, remote cache or authority model. Each adapter retains at most one tuple
of generation ID, BLAKE3 digest of the complete record bytes, and schema version.
Every page still reads and hashes the authoritative record. A miss fully decodes
the existing record, checks its manifest generation, and only then caches the
schema. Changed/corrupt bytes cannot use a generation-only stale hit; failures
are not cached. Poisoned cache synchronization fails closed. No history payload,
graph snapshot, workflow state or unbounded generation map is retained.

This changes no public contract, migration, canonical storage, ranking or
verification mode. Full integrity checks keep their complete history decoding.
Tests must cover repeat hits, content changes, generation separation, replacement,
failed decode/recovery, and database corruption after a warmed page lookup.
Backend conformance and a measured whole-corpus rerun remain required before
claiming correctness breadth or an end-to-end speedup.

Implemented in the Turso adapter's historical node-page/search paths. The focused
single-entry cache test passes; the existing history-integrity fixture now warms
both paths, corrupts the authoritative old record and checks both fail before
reopening/full audit. All 40 Turso unit tests and the shared historical-node-page
conformance scenario pass (7.56 seconds for the latter); strict all-target Clippy
is being rechecked after replacing assertion-in-Result checks and unrelated
binding shadowing with explicit fixture errors and distinct names.
The complete contributor gate and whole-Sim rerun are in progress. No measured
end-to-end speedup is established yet; every page still reads/hashes the record.
The first complete gate stopped at strict fixture binding-shadow lint checks;
those names were corrected and the terminal gate restarted. The failed gate is
not correctness or speedup evidence.

Whole-Sim rerun `context-20261001T144204.770892Z-uncommitted` completed with
identical exact/stemmed/fused target ranks to the pre-cache fusion run
`context-20261001T140601.449126Z-uncommitted`; unchanged production acceptance
still misses TypeScript/Python/Bash. Test elapsed time was 132.36 seconds versus
206.28 seconds in that earlier run. This is a single directional comparison,
not an isolated speedup measurement: the rerun overlapped the complete CI gate,
and filesystem/cache/load conditions were not controlled. Every page still reads
and hashes its complete history blob. No relevance improvement is claimed.
The corrected complete `cargo make ci` passed in 200.38 seconds, including strict
workspace Clippy, dependency policy, all workspace/doc tests, all 16 backend
conformance scenarios and API documentation. No required migration was added.
