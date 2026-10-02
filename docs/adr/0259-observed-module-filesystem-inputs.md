# ADR 0259: Observe module filesystem inputs without replacing resolution

Status: accepted

Oxc's directory dependency-context API cannot replace `resolve_file`: it loses
automatic tsconfig discovery. Wrap its supplied `FileSystem` instead, delegating
all operations unchanged and recording every attempted path, including missing
paths and arbitrary extended config names. Both import/CommonJS clones share
the observation set. Expose a sorted adapter-level path snapshot, synchronized
with resolution and refresh; paths are host data, not core graph identities.
Poisoning fails closed. Keep observations across cache refresh so a host does
not lose its previous dependency coverage before replacement is persisted.

This is the observation phase, not complete invalidation. Host integration must
persist a versioned dependency inventory through existing durable record/CAS
ports, compare contents and missing/existing state before no-op planning, and
rebuild coverage on provider-policy changes and restart. Metadata existence,
symlink targets, outside-root inputs, permissions, and changes during publication
need explicit fingerprint semantics. Do not claim config-only edits are handled
until unchanged-caller, restart, missing-to-present, and extended-config lifecycle
fixtures pass. No new parser, database schema, or executable runtime is required.

Implementation: the Oxc adapter now delegates through an observed filesystem
and exposes sorted `observed_paths`. All 22 ECMAScript tests pass. Expanded
tracking coverage verifies auto-discovered and extended configs, missing target
probes, and coverage retained across refresh. Strict adapter Clippy passes after
using explicit ignored error bindings for the project's lint policy. No full CI
or production host invalidation is claimed for this observation slice.

Package coverage: the additional native fixture records `package.json`, both
import/require targets, and a missing auto-discovery `tsconfig.json` probe in one
sorted/deduplicated inventory. A package-only exports edit followed by explicit
refresh changes both mode results. All 23 ECMAScript tests and strict adapter
Clippy pass. This does not prove no-op planning notices the edit automatically.
