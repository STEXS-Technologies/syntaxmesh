# ADR-0182: Stop HTTP query admission at shutdown

Status: Accepted

## Decision

Borrow Shardline server app.rs signal-first lifecycle sequencing: shutdown
policy begins when the injected signal resolves, not when the server starts.
Close the HTTP host shared query semaphore before Axum starts graceful draining.
New query attempts through any clone then return the existing 503 response.
Already accepted blocking workers retain their owned permits and finish normally.
Shutdown is terminal for a host instance; reopening constructs fresh admission.

Do not copy Shardline's optional timeout verbatim. Dropping an HTTP request or
serve future cannot cancel a started Rust database worker. A timeout must account
for those workers and durable workflow reconciliation before SyntaxMesh can
promise bounded process termination. This read-only host does not run daemon
indexing workflows and does not close that acceptance gate.

## Verification

Extend the existing TCP fixture to retain a host clone, trigger injected shutdown,
await the server, and verify the clone refuses new queries. Existing cancellation
tests must continue proving that a running blocking worker retains its permit.

Full cargo make ci passed on 2026-09-30 in 121.02 seconds. The HTTP TCP fixture
checks every retained clone rejects query admission after injected shutdown;
existing worker-cancellation/permit-retention coverage remains green.
