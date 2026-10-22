# ADR-0343: Ontology registry byte admission

Status: implemented.

Require positive encoded-byte limits for ontology registry construction and rule
binding validation. Reuse ADR-0342's counting writer before copying registry
strings or inspecting oversized rule metadata. Registered evaluator and Query
paths use `Limits::input_bytes` for rule admission before retained fact loading.
Keep count limits and explicit rule/binding work budgets independently enforced.

Use a namespace-to-version-set map for borrowed lookup, avoiding per-binding
string copies. This admission bound is not an exact heap bound and does not
bound inputs already allocated by callers or authenticate ontology definitions.

Verification: fifteen inference unit tests and Rust integration pass. Exact byte
boundary acceptance, one-byte-under rejection and zero-budget rejection cover
registration and validation. Four-backend registered inference fixtures pass;
strict all-target inference/Query/SQL Clippy passes. No registry persistence or
ontology compatibility implementation is implied.
