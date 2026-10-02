# ADR-0083: Compile token-budgeted evidence context in the Rust query layer

- Status: accepted
- Date: 2026-09-29

## Context

The v0.1 definition requires token-budgeted context, and the product contract
calls for source snippets with exact file/line evidence, relationship context,
ambiguity warnings, an omitted-context summary, and an explicit token count
([SyntaxMesh §§17–17.1](../syntaxmesh-todo-extensible-opensource-v5.md)). The
current read-only MCP host exposes search and graph traversal, but deliberately
makes no context-budget claim ([ADR-0063](0063-read-only-mcp-host.md)).


SyntaxMesh must remain executable in Rust and analyzable-language/runtime
agnostic. Core, public DTO, and thin protocol crates cannot read host files or
depend on workflow, database, or host APIs. Token counting is model/tokenizer
specific, and source text is not part of the canonical graph store.

## Decision

1. Implement context selection and packing as deterministic Rust logic above
   the existing generation-pinned query service. Keep graph discovery in the
   existing query/store path; do not add a second graph or an LLM planner.
2. Make the request generation-pinned and require an explicit token limit.
   Accept a natural-language query and optional stable seed IDs; initial
   retrieval uses existing lexical search and bounded graph traversal. Ranking
   is deterministic and records its evidence basis: lexical/symbol match,
   graph distance/relation, source-backed status, and available provenance.
   Embedding and model-generated ranking are deferred.
3. Keep source retrieval and exact token measurement injected. The Rust query
   layer receives a source-content provider and a tokenizer/counting provider;
   neither provider is persisted or imported by core, DTO, store, or protocol
   crates. A consumer that cannot supply a token counter cannot claim an exact
   token-budgeted result.
4. Treat source snippets, signatures, graph-path records, and deterministic
   summaries as complete candidate items. Select in the documented quality
   order—exact source evidence, signatures, graph paths, then summaries—and
   choose higher-ranked items before lower-ranked items within a class. Never
   truncate arbitrary bytes or source text to make a candidate fit. Count the
   serialized response envelope and omission/ambiguity metadata against the
   same budget; omit an item when it will not fit and report why it was
   omitted. If required response metadata itself cannot fit, return a typed
   budget error rather than an over-budget pack.
5. Return explicit generation/worktree identity, token count, selected items
   with file/line and graph evidence, ambiguity notices, and a bounded summary
   of omitted candidates. The pack is an evidence projection, not a new source
   of truth and not a semantic conclusion.
6. Keep the MCP adapter thin: it supplies the repository-scoped source reader
   and configured tokenizer, calls the shared engine/query operation, and
   serializes the versioned DTO. No MCP/Tokio/filesystem dependency enters the
   Rust query or engine contracts.

## Alternatives considered

- Put source text in the canonical graph/database: rejected because it couples
  graph persistence to workspace file access and duplicates source content.
- Count UTF-8 bytes or use a fixed characters-per-token estimate: rejected
  because it cannot guarantee a model-specific token ceiling.
- Truncate rendered JSON/source text until it fits: rejected because it can
  destroy spans, syntax, or evidence while pretending to preserve provenance.
- Use an LLM or embedding service to select context: deferred; the deterministic
  baseline must be useful and auditable without another runtime or service.
- Implement the feature inside the MCP host: rejected because embedders and
  future hosts must share identical context semantics.

## Consequences

- The first implementation needs versioned context DTOs, a runtime-neutral
  source/token counter boundary, a deterministic selector, and an MCP tool.
- Exactness is relative to the injected tokenizer's declared model/config; the
  pack must identify that tokenizer/config so callers can interpret its count.
- The selector must be tested for deterministic ordering, exact budget
  accounting including envelope overhead, complete source spans, omission
  transparency, generation pinning, ambiguity, and provider failures.
- Candidate quality is initially lexical and graph-based; the ranking is not a
  claim of semantic relevance or completeness.

## Verification required before v0.1 completion

- Cross-check every included source span against the supplied source content
  and report one-based line ranges.
- Prove the final serialized pack count is at or below the requested budget
  using a test tokenizer; reject budgets too small for the required envelope.
- Verify deterministic selection and output across repeated runs and all
  supported stores, including ambiguous search results and stale generations.
- Exercise the public engine API and MCP tool against the same fixture and
  verify they return byte-equivalent pack DTOs.
- Run representative repository retrieval fixtures and measure omitted versus
  included evidence; do not call the context compiler complete based on unit
  tests alone.

## References
