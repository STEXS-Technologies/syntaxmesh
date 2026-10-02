# ADR-0309: Probe native page-cache sizing before changing store policy

Status: diagnostic verified; production cache policy unchanged.

ADR-0308 fails to remove repeated full-record SQL processing. The pinned Turso
driver supports SQLite-style page-cache sizing; Shardline's local SQLite setup
does not explicitly tune it. Do not copy a setting that Shardline does not use
or introduce an application cache based only on intuition.

Add an ignored Rust diagnostic beside the existing schema-cache tests. Use
an isolated tempfile database, one 33,580,768-byte BLOB matching the measured Sim
record size, and repeated scalar-row payload reads with the default-style
2 MiB cache and a 64 MiB native page-cache suggestion. Report elapsed time and
observed pragma values, not asserted speedup. This is a driver microdiagnostic,
not a repository retrieval benchmark, memory bound or production policy.
Keep all normal store settings, integrity checks and public contracts unchanged.

The pinned driver's `CacheSize::DEFAULT` is `-2000`. The diagnostic verifies
WAL mode and reads back both requested cache settings. One disk-backed debug
sample reports five reads in 1,230,441 us at `-2000` and 1,075,549 us at
`-65536`; the preceding non-WAL sample is not used for the WAL conclusion.
The larger suggestion does not remove full-record processing and gives only
a modest local timing difference while increasing memory allowance. No
production cache-size change is justified by this sample. Order, warm-up and
host effects are not isolated; this does not prove an I/O or CPU diagnosis.

The explicit ignored diagnostic passes. The regular library suite passes
47 tests with this diagnostic ignored, and strict all-target/all-feature Turso
Clippy and formatting pass. Run it with:

```sh
cargo test -p syntaxmesh-store-turso --all-features native_history_payload_page_cache_probe -- --ignored --nocapture
```

Use a suitable `TMPDIR` for quota-limited systems. The fixture owns and removes
only its temporary directory. Keep investigating a bounded historical scan
that validates its generation once within one operation, rather than weakening
separate-call authoritative validation or tuning caches without evidence.
