# ADR-0173: Bounded extension IPC frames

- Status: accepted
- Date: 2026-09-30

## Decision

Implement the first external-producer wire codec in a separate Rust
`syntaxmesh-extension-ipc` adapter, not in core, the SDK, or runtime protocol.
Reuse the SDK's `FactBatch` and validation rather than define another fact model.
No producer executable is launched and no graph store is opened by this adapter.

Each frame has a twelve-byte header: ASCII `SMEX`, a big-endian u32 wire version
(initially 1), and a big-endian u32 payload byte length. The payload is exactly
one UTF-8 JSON `FactBatch` object using existing serde field representations.
This is framed local IPC, not NDJSON. JSON is an interoperability boundary, not
canonical storage or an internal Engine service handoff. A stream may contain
multiple frames; clean EOF is legal only before a header begins. Partial frames,
unsupported versions, zero lengths, oversized lengths, malformed JSON, and
invalid batches fail. A caller must discard a failed stream rather than attempt
resynchronization. Caller-supplied byte limits cannot exceed the 4 MiB hard cap.
Check the header and limit before allocating or consuming payload bytes.

Adapt Shardline's fixed-width binary header and `read_exact` pattern
(`shardline-xet-core/src/xorb_object/xorb_object_format.rs`); do not copy its
storage format, compression, runtime, or unrelated dependencies. Use existing
workspace serde/serde_json rather than add another serialization framework.

The codec does not authorize producers, guarantee authenticity, enforce graph
referential integrity, or commit generations. Engine ingestion retains those
responsibilities. A deployment host must supply timeouts, peer authorization,
stream ownership, receipts, and backpressure. Blocking `Read`/`Write` are adapter
ports, not an Engine runtime requirement. The complete out-of-process extension
gate remains open until end-to-end host/Engine equivalence and failure-isolation
fixtures exist.

## Verification

Seven codec tests cover a hand-authored external JSON frame, repeated frames,
one-byte fragmented reads, every partial-frame truncation, header rejection
without payload consumption, exact-limit acceptance, malformed JSON, invalid
manifests, no-write validation/size rejection, interrupted reads, and partial
write failure. The initial codec-only implementation passed `cargo make ci`
(129.22 seconds), including strict workspace
Clippy, architecture boundaries, out-of-tree Rust extension conformance,
migrations, dependency audits, workspace tests, and documentation builds.

Three subsequent Engine integration fixtures reuse the existing extension batch
and runtime observation helpers. Populated nodes, an external edge, payloads,
provenance, and a client-probe observation preserve exact SDK values and produce
equal logical snapshots through framed/File and embedded/InMemory ingestion.
Current and historical snapshots survive File-store restart, with both durable
Penelope completions present. An invalid wire namespace creates no workflow
records, leaves the accepted snapshot unchanged, and does not prevent a later
valid producer from publishing. On Unix, a socket-pair/thread fixture transports
a populated batch over an actual local stream before engine publication.
All six extension-ingestion tests, focused strict Clippy, and architecture checks
pass on this follow-up. This is stream/Engine equivalence, not an independently
executed producer or deployment host: peer authorization, protocol receipts,
production deadlines, and process-failure conformance remain open.
