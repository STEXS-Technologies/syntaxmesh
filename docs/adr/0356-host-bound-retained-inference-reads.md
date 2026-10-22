# ADR-0356: Host-bound retained inference reads

Status: initial implementation in progress. Pure transport records, shared host
admission/conversion, and opt-in HTTP/MCP routes/tools are implemented. DTO and
host tests pass; the HTTP TCP rejection test passes; default and feature-enabled
MCP builds and strict Clippy pass. Positive transport pagination/history tests
and current full contributor CI remain outstanding.

## Context

ADR-0355 exposes local CLI inference workflows and retained reads. Its explicit
database paths and caller-supplied origin receipts are local operator inputs,
not appropriate remote filesystem authority. The public API model currently has
no database dependency; retained stream types currently live in the downstream
SQLite adapter. Importing that adapter into public DTOs would violate the pure
crate boundary.

## Decision

Place shared operator binding and admission in a dedicated
`syntaxmesh-inference-host` crate, not in the pure DTO crate or duplicated across
HTTP/MCP. Its storage-aware admitted requests are host-local, not wire records.

Deliver read-only retained inference through HTTP and MCP using operator-installed
bindings. Each binding selects an existing downstream database and a complete
immutable origin receipt and belongs to the host's repository/worktree scope.
Clients select a binding identifier and an explicit retained generation, never a
filesystem path or a replacement origin. Reject unknown and foreign-scope bindings
before storage access. Binding identifiers and origin receipts are selections,
not authentication credentials; existing host access controls remain necessary.

Define versioned transport-only records in `syntaxmesh-api-model`, using pure
core coordinates and owned bounded response data. Do not add database, workflow,
host or transport dependencies there. Perform explicit adapter-to-DTO conversion
in the host integration, retaining semantic identities, exact premise versions,
derivation classification and checkpoint coordinates rather than exposing only
unattributed text. Do not move SQLite implementation types into core merely to
make serialization convenient.

Expose checkpoint lookup and cursor-based claim pages first. Require positive
independent claim, payload-byte, premise and tree-page limits, capped by operator
policy. Bound request and cursor decoding before storage access. A cursor must
remain bound to the configured origin, complete historical selection and active
set root; the transport cursor additionally carries the binding identifier so
aliases cannot accidentally accept one another's continuation. Altered or
cross-binding cursors fail closed. Use fixed-width unsigned wire counts and
checked conversions to adapter `usize` limits, not architecture-dependent wire
admission. Return an explicit
unretained selection rather than silently substituting the newest checkpoint.

Reuse the engine retained-read service and downstream validation. Reads must not
scan canonical history, replay rules, migrate schemas, recover prepared jobs,
initialize streams or synchronize generations. Remote mutation workflows are
not included in this decision. Keep blocking storage work outside asynchronous
executor threads and preserve existing bounded host admission and ownership
checks. Publish no successful partial page on budget or validation failure.

## Required evidence

Pure DTO dependency checks and serialization round trips must pass. HTTP and MCP
must return equivalent historical checkpoint/page coordinates and drain a
multiple-claim baseline without duplicates or omissions after later retraction.
Tests must reject unknown/foreign bindings, substituted cursors, oversized input,
zero/excessive limits, unavailable storage and unretained selections without
implicit creation, migration or canonical mutation. Retained reads must succeed
without a canonical replay, including while canonical writer ownership is held.
Disabled inference support must leave existing host behavior unchanged.

Targeted tests, strict Clippy, architecture checks and full contributor CI are
required before claiming this delivery complete. This read boundary does not
complete higher reasoning layers, retrieval quality or the v0–v5 roadmap.

## Initial implementation

The API model now declares versioned checkpoint/page requests and responses,
fixed-width limits/counts, full historical selections, binding-scoped cursors and
exact claim witness fields with explicit derivation classes. Request records
reject unknown fields; deserialization is not host admission, so version,
positive/capped limits, binding and cursor validation still belong to the host.
No dependency was added. Three serialization tests pass in a separate target
directory: fixed-width request limits and unknown-field rejection, explicit
unretained checkpoints and derivation wire names, and a complete page preserving
exact premise versions, birth versus selected generations and cursor coordinates.
Formatting, diff checks, a fresh architecture gate and strict all-target Clippy
pass. The accumulated CI run is still building its native dependency;
no full CI success is claimed for these records.

The dedicated host crate now registers at most 256 immutable operator bindings,
checks repository/worktree scope and duplicate identifiers, and admits checkpoint
and page requests without filesystem access. Page admission checks the version,
binding, four positive independent policy ceilings and checked `usize` conversion.
It binds cursors to both the operator identifier and complete stream/selection;
retained root/key validation remains the materializer's responsibility. Three
host tests pass in the separate target directory, including cross-alias
and foreign-coordinate rejection. A fresh architecture gate, formatting and diff
checks and strict all-target host Clippy pass. The lint run identified one test
variable shadow, fixed by renaming without an allowance. These are
admitted host-local requests, not yet executable HTTP/MCP endpoints.

Explicit checkpoint/page conversion now preserves fixed-width counts, complete
selection and cursor coordinates, every witness field and an exhaustive
derivation-class mapping. Conversion documents that it is not storage validation,
request admission or authentication. All four host tests pass, including a
field-by-field witness conversion fixture with distinct birth/selected generations.
Strict all-target Clippy for this conversion slice passes; full CI remains
pending.

Initial opt-in HTTP wiring exposes POST checkpoint and claims-page routes with
16 KiB request limits and an explicit scope-checked registry builder. It admits
requests before the existing bounded blocking engine-read path, reuses engine
retained reads and shared conversion, returns an explicit null checkpoint or
page 404 for unretained generations, and introduces no mutation endpoints. The
feature-enabled HTTP crate check and strict all-target Clippy pass. A real TCP
boundary test passes for unconfigured inference, client-selected path fields,
unknown bindings, excessive independent limits, oversized JSON and missing
downstream storage without creation; rejected requests preserve canonical
selection.

MCP now has opt-in checkpoint and claims-page tools using the same admission,
conversion, worker ownership and bounded semaphore. The feature-enabled and
default-feature MCP checks and strict all-target Clippy pass. Feature-disabled
tools return an explicit disabled-build error. MCP runtime invocation and
positive pagination/history behavior have not yet been verified. Remaining
transport evidence includes equivalent positive retained pagination, later-
retraction history, malformed cursor delivery and client-visible unretained
selection behavior. Therefore the host delivery is not complete, and current
full contributor CI has not been rerun after the final MCP fixes.
