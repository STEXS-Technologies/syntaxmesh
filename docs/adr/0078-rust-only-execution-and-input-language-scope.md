# ADR-0078: Keep execution Rust-only and bound analyzed source formats

- Status: accepted
- Date: 2026-09-29

## Context

SyntaxMesh is a Rust-native, runtime-agnostic engine. “Runtime-agnostic” means
the engine can be embedded by different host applications; it does not mean
SyntaxMesh launches every runtime represented in a repository. The accepted
Phase 8 roadmap already defines language packs as Rust implementations and
names Rust, TypeScript/JavaScript, Python, and Bash as analyzed source formats.
It explicitly excludes other source languages unless product scope is revised.
An older language-support-order section still listed Go and Java, creating a
conflicting roadmap.

## Decision

1. SyntaxMesh implementation, build, indexing, and query execution are Rust.
   Rust is the only runtime the project depends on or launches for its own
   operation.
2. Initial code-source inputs are Rust, TypeScript/JavaScript, Python, and Bash
   scripts. Markdown and plain-text documentation formats are also inputs.
   These are parsed or scanned by Rust code; their language runtimes are not
   invoked.
3. “Node” and “Python” module-resolution profiles are static analysis
   strategies implemented in Rust, not dependencies on Node.js or Python
   runtimes.
4. Adding another analyzed source language or executable runtime requires an
   explicit product-scope revision. Do not add one implicitly as part of
   parser, integration, benchmark, or roadmap work.
5. This constrains first-party code and processes in the SyntaxMesh workspace;
   it does not prevent out-of-process consumers or extension producers from
   speaking the versioned public protocol in another language. Such clients
   remain separate projects and are not launched by SyntaxMesh.

## Consequences

- Runtime-neutral APIs remain host-independent Rust contracts, not a mandate
  to support multiple implementation runtimes.
- The boundary does not narrow the public extension/runtime-observation
  protocols or the languages external consumers may use to implement them.
- Existing Rust-backed extractors and resolution profiles remain in scope;
  the older planned Go and Java language-pack entries are removed.
- Documentation examples may mention other languages when describing external
  systems, but must not imply those languages are SyntaxMesh implementation or
  execution runtimes.

## Verification

- The source roadmap and v0 plan state this boundary consistently.
- The default scanner accepts only the in-scope source/document formats.
- `cargo make architecture` parses Rust files under `crates/` and audits direct
  `Command::new` expressions. It permits Git metadata lookup and Rust workspace
  binaries used by integration tests; dynamic executable names fail closed.
  This guards the product crates against implicitly adding language runtimes.
