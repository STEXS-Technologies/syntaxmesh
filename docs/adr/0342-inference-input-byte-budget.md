# ADR-0342: Inference input byte budget

Status: implemented for evaluator input preflight.

Add a mandatory positive `Limits::input_bytes` bound for serialized inference
input: selected node identities, generation-labeled premises and the complete
rule set. Preflight through a counting writer without allocating a full encoded
buffer; fail before rule ordering, adjacency allocation or claim encoding when
the encoding exceeds the bound. Single-rule and registered paths share this gate.
This is in-memory size accounting, not a storage or communication format.

Reuse Serde already present in the pure crate. Bound fact and rule counts before
byte accounting. A byte budget is not an exact heap bound or a replacement for
depth/work/output limits. Callers have already allocated their input; host
decoding and premise-reader allocations need independent limits. No inference
wire format has been published, so this required field changes only the optional
Rust evaluation configuration.

Verification: fourteen inference unit tests and Rust integration pass. Exact
encoded-size acceptance, one-byte-under rejection, zero budgets, oversized rule
identifiers and extension payloads are covered. Existing four-backend lifecycle
fixtures pass with explicit input budgets. Strict inference/Query/SQL all-target
Clippy passes. Registry construction and standalone witness readers/verifiers
retain their own count bounds; this does not add byte accounting to those APIs.
