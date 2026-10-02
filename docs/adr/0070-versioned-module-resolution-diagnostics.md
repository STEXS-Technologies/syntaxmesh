# ADR-0070: Retain module-resolution outcomes as versioned graph facts

- Status: accepted
- Date: 2026-09-28

## Context

ADR-0067 keeps extracted import/export occurrences immutable and publishes
successful resolution as separate provenance-backed edges. The provider also
returns unresolved, ambiguous, and invalid outcomes, but the indexer currently
discards them. Users therefore cannot tell whether an occurrence was
considered, why it has no `ResolvesTo` edge, or how that result changed between
accepted generations. An absent edge is not enough to distinguish an
unresolved request from a disabled provider or an occurrence that was never
processed.

Resolution outcomes depend on an explicitly selected provider and repository
configuration, so they are derived evidence rather than source facts. They
must not mutate Import/Export payloads or be confused with static extraction.
At the same time, a durable graph must retain the outcome accepted for each
generation; recomputing it after restart could observe a different filesystem
or resolver configuration.

## Decision

1. Persist each non-success provider outcome as a dedicated typed
   `ModuleResolutionDiagnostic` graph node connected to its source occurrence.
   Supported statuses are unresolved, ambiguous, invalid, and resolved-but-not
   present in the indexed module inventory. A successful indexed resolution
   remains represented by the existing `ResolvesTo` edge and creates no
   diagnostic.
2. Append the new node, relation, and evidence variants to their enums so
   existing bincode variant ordinals remain stable. The diagnostic node stores
   the source occurrence ID, status, and normalized repository-relative
   candidates where applicable. Its identity is stable for one occurrence and
   provider identity/settings fingerprint, allowing normal temporal fact
   history to show result changes and removal/reintroduction.
3. Add a distinct `ResolutionDiagnostic` evidence class. Its provenance uses
   the source location of the diagnosed occurrence and the resolver producer
   identity. Import/Export source nodes and their extraction provenance remain
   byte-for-byte unchanged. Do not persist host-specific error strings or
   absolute filesystem paths; typed status and repository-relative candidates
   are the portable diagnostic contract.
4. Publish diagnostic-node changes atomically with resolution edges through
   the existing graph delta and Penelope workflow. When a provider is
   configured, every accepted import/re-export occurrence is represented by
   either a successful `ResolvesTo` edge or a diagnostic node. When resolution
   is disabled, remove resolver-owned edges and diagnostics without changing
   source occurrences. The store does not re-run a provider during queries or
   recovery.
5. Expose diagnostics through a generation-pinned Query API and a read-only
   CLI query/export path. Durable stores use the existing indexed node-kind
   projection and assign a new database-private code to diagnostic nodes.
   SQLite/Turso append a migration version fence; no diagnostic backfill is
   needed because this node kind could not exist in older databases. New node
   upserts maintain the projection transactionally. File/InMemory remain
   behavioral reference implementations.
6. Teach the Oxc adapter to map resolver errors to stable typed outcomes
   without embedding machine-specific paths. Other providers retain authority
   over their deterministic, runtime-neutral outcome classification.

## Alternatives considered

- **Infer unresolved status from the absence of `ResolvesTo`:** rejected
  because no edge does not prove that resolution ran and loses ambiguity,
  invalidity, and provider-disable distinctions.
- **Return diagnostics only in the indexing call result:** rejected because
  outcomes would disappear after restart and could not participate in
  historical graph queries.
- **Mutate the source Import/Export node:** rejected because it couples source
  evidence to resolver settings and would rewrite extraction identity/history.
- **Persist free-form resolver error strings:** rejected because they can
  include absolute paths, vary across runtimes, and make canonical graph state
  host-dependent.
- **Add a diagnostics table beside graph facts:** rejected because it would
  create a second versioning/query/history mechanism for evidence that already
  fits the canonical graph fact lifecycle.

## Consequences

- A pinned query can explain why a typed source occurrence has no resolution
  edge, and historical fact/node queries retain previous outcomes without
  replaying indexing requests.
- Resolution diagnostics are derived graph facts with explicit provenance;
  they are not source syntax and do not claim symbol/member binding.
- The new node kind requires explicit SQLite/Turso migration version bumps and
  an update to the indexed node-kind projection. Enum variants must be
  appended to preserve existing bincode encodings.
- Provider implementations must return deterministic outcomes and normalized
  repository-relative candidate paths. Oxc's adapter is responsible for
  avoiding ambient path leakage in its diagnostic mapping.

## Verification required

- Missing, ambiguous, invalid, and non-indexed-target outcomes create the
  expected typed diagnostic; an indexed target creates only a `ResolvesTo`
  edge.
- Changing an outcome updates the diagnostic at a new generation; removing a
  target retracts the edge and records its new outcome. Historical queries
  return the prior state after restart without invoking the provider.
- Disabling the provider retracts derived resolution state while preserving
  source occurrence identity, payload, span, and extraction provenance.
- SQLite and Turso migration/backfill, indexed lookup, restart, integrity, and
  cross-backend conformance cover the new node kind and relation.
- Graph root, node history, fact history, deterministic exports, and workflow
  recovery remain consistent with the new diagnostic facts.

## Implementation status

Implemented on 2026-09-28. `NodeKind::ModuleResolutionDiagnostic` and the
`ResolutionDiagnostic` evidence/relation variants are appended to preserve
existing bincode ordinals. Diagnostics keep only portable typed status and
normalized repository-relative candidates; provider error strings are not
persisted. SQLite schema v17 and Turso schema v20 fence the new representation;
the existing indexed `node_kind` projection selects diagnostics without a new
table or fact family. The Indexer atomically publishes diagnostic nodes,
provenance, and occurrence edges alongside successful resolution edges, and
retracts them when a target resolves or the provider is disabled. The
generation-pinned Query API and `resolution-diagnostics[-turso]` CLI command
expose current outcomes. Tests cover all four statuses, candidate
normalization, outcome replacement and node history, Turso restart/CLI query,
cross-store fact round trips, migrations, and indexed Turso inventory reads.
