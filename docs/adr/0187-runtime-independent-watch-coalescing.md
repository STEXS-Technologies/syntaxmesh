# ADR-0187: Runtime-independent watch coalescing

Status: Accepted

## Decision

Add a dependency-free syntaxmesh-watch scheduling component, separate from
filesystem notifications, clocks, executors, storage and Engine workflows.
Hosts provide elapsed monotonic time. Notifications mark one pending inventory
rescan rather than retaining an unbounded path queue. The existing scanner and
incremental indexer remain responsible for discovering changes and removals.

A batch becomes due after a quiet interval or a maximum interval measured from
its first event, whichever comes first. New events cannot postpone that maximum.
Consuming a due batch clears it; events received during indexing form a subsequent
batch. Overflow/rescan notifications use the same mark operation. Hosts must
retain such notifications, retry failed indexing and coordinate mutations under
the ownership lease; the policy itself does not execute or acknowledge indexing.

No matching filesystem scheduler was found in Shardline/Penelope/StateChronicle.
This component introduces only the missing policy and does not duplicate their
storage, recovery, or workflow mechanisms. It does not yet provide a watch CLI,
OS notification adapter, daemon, shutdown protocol, or IPC.

## Verification

Five deterministic tests cover invalid intervals, quiet-window merging,
maximum-delay enforcement under continuous events, subsequent/retry batches,
out-of-order timestamps, and extreme durations without clock sleeps. Package
tests, strict all-target Clippy, and the workspace architecture gate pass.
