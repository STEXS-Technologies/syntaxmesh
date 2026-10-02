# ADR 0255: Rust block item value constraints

Status: accepted


All 30 extractor tests and strict extractor Clippy pass. Expanded File and
verified-Turso lifecycle tests cover parameter and hoisted const constraints
through definition edits, reopening, unchanged-caller re-resolution, cached
rescans, and retained history. Full CI for producer 0.14.0 passes in 213.86
seconds, including strict workspace Clippy, independent extension imports,
migration registries, all 16 backend-conformance scenarios, and documentation.
The manual representative context-retrieval evaluations remain ignored by CI;
this run does not establish the broader relevance quality bar.
