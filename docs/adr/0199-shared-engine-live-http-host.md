# ADR-0199: Shared-engine live HTTP host

## Decision

Add `SyntaxMeshHttp::from_shared_engine` for trusted Rust hosts that already own
an `Arc<Mutex<SyntaxMeshEngine<TursoGraphStore, RustExtractor>>>` and explicit
repository/worktree scope. Read operations and host-owned publications use the
same lock and connection. The constructor requires a published scoped graph.
Default queries select the current generation under that lock; historical
queries retain their explicit selection. Response labels use ADR-0198's query
snapshot, and readiness returns diagnostics and generation from one lock hold.

Standalone `open` retains startup pinning and stale-host rejection. The new
mode does not silently reopen external databases or permit competing writers.
The owning host must retain a writer lease, run durable recovery, and perform
mutations via the Engine. HTTP remains read-only and admission-bounded.

Keep a shared Engine reference outside the async runtime until shutdown so
Turso's internal runtime is dropped synchronously. This first shared constructor
uses the current HTTP Rust extractor type; generic extractor hosting, daemon
IPC, CLI attachment, analytics scheduling, and bounded mutation shutdown remain
open. Blocking writes serialize reads; this does not promise parallel serving.

## Verification

Serve a real loopback listener, publish an edited Rust source through the same
Engine lock, and compare default searches, retained historical searches,
generation labels, and readiness after publication without restarting HTTP.

Verified: all seven HTTP tests pass. The live fixture holds the reusable writer
lease, performs startup recovery, enables StateChronicle for both publications,
and audits the retained verification history. Strict HTTP Clippy, formatting,
and architecture checks pass. Full workspace CI was not rerun for this change.
