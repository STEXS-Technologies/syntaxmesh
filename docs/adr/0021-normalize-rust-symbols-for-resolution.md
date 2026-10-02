# ADR 0021: Normalize Rust symbols only for resolution matching

- Status: Accepted
- Date: 2026-09-27
- Deciders: SyntaxMesh maintainers

## Context

Rust impl method definitions are currently represented by token-stream names,
which can include formatter whitespace and generic arguments (for example,
`Service < T >::run`). A source call path such as `Service::run` omits those
arguments. Exact lookup therefore misses the definition and may incorrectly
fall back to a repository-wide terminal-name match, producing an avoidable
ambiguity. Reference spelling and source spans are evidence and must remain
unchanged.

## Decision

Normalize symbol lookup keys by removing token whitespace and balanced generic
argument lists. Apply the same normalization to extracted definition names and
reference target spellings. Use exact normalized qualified matches before the
existing conservative terminal-name fallback. If normalization yields
multiple candidates, preserve the reference as ambiguous. Do not rewrite stored
node names, stable IDs, reference labels, or source spans.

## Consequences

- Qualified calls to generic impl methods can resolve without a terminal-name
  guess.
- Existing graph identity and serialized records do not change.
- This is syntax-level normalization, not Rust name or type resolution. Imports,
  lexical scope, trait dispatch, macros, and compiler-assisted lookup remain
  outside this decision; uncertain cases stay explicit.
- The normalization is confined to resolver lookup keys and can be replaced
  with richer language-specific symbol identities later without changing the
  occurrence evidence contract.

## Verification

- Resolver fixture proves a generic-formatted definition matches its qualified
  call path.
- Extractor fixture proves generic impl method names are retained.
- Indexer fixture proves the Rust source call produces the expected `Calls`
  edge to the generic impl method.
