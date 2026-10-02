# ADR-0053: Incremental StateChronicle head validation

- Status: accepted
- Date: 2026-09-28

## Context

SyntaxMesh's opt-in StateChronicle adapter re-read, decoded, and re-hashed the
entire per-worktree generation chain on every verified publication. As history
grew, a normal append therefore cost O(H) in retained history, and a sequence
of H publications could perform O(H²) verification work. Shardline's reliability
journals use a useful two-level pattern: normal mutations validate a
hash-checked durable head, while an explicit audit replays the complete chain.

StateChronicle remains the generation commitment/digest provider. Its role is
separate from SyntaxMesh's temporal graph indexes and persistent generation
roots. This change only separates routine append validation from deep audit.

## Decision

1. Extend `DurableRecordStore` with a deterministic latest-record-by-prefix
   operation. The default implementation may scan for compatibility;
   InMemory/File use their ordered map and SQLite/Turso use their primary-key
   range indexes.
2. For a normal append, `StateChronicleVerifier` reads and validates only the
   latest record's key, ordinal, scope, manifest digest, record digest, and
   requested parent link, then compare-and-swaps the next ordinal. This is
   independent of retained history depth for indexed durable stores.
3. Exact retry of the head generation is O(1). Requests for an older generation
   and CAS-race recovery use the full replay path so existing idempotency and
   concurrent-writer behavior remain intact.
4. Expose an explicit full-history verification operation that checks sequence
   keys, every manifest and record digest, scope, uniqueness, parent links,
   and hash-chain continuity. Routine append validation is not a substitute
   for this audit: corruption in an older record is detected by full replay,
   not by checking only the current head.
5. Keep the feature opt-in, store its records through `DurableRecordStore`,
   and do not introduce StateChronicle or workflow dependencies into the store
   crate, core, DTOs, or runtime protocol.

## Consequences

- Opt-in verified publication no longer replays all prior StateChronicle
  records on each normal append; SQLite/Turso and ordered reference stores can
  retrieve the latest record using their key index.
- Operators/hosts retain an explicit deep-replay API for full-chain assurance.
- This is local integrity checking, not a signature or defense against an
  actor able to rewrite both data and its authority.
- The `DurableRecordStore` trait gains a source-compatible default method;
  custom stores remain correct but should override it to provide indexed
  performance.
