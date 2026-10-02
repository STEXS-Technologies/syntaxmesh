# ADR 0281: Rust function-definition evidence without changing owner lookup

Status: accepted, 2026-10-02


The extractor builds declaration owner maps from these identifier locations.
`declaration_at`, trait defaults and local containment depend on exact narrow
keys. Replacing ranges before reference and containment extraction would break
ownership. Keep those internal locations through all owner-dependent passes;
only then enrich emitted Function nodes with their AST definition ranges.
Named functions, inherent/trait implementation methods and trait required/default
methods need coverage, including local nested functions. Required trait methods
cover their signature and terminator without inventing a body. Non-function
declarations and call/import/reference occurrence ranges remain unchanged.

Implementation must preserve stable IDs, qualified names, duplicate ordinals,
call owners and existing source-prefix/UTF-8 checks. Bump the Rust producer
version to invalidate unchanged inputs. Existing retained generations keep their
old evidence. No DTO, schema, storage, dependency or runtime change is required.
The existing exact-budget packer may omit a complete definition that cannot fit;
this is not a candidate relevance fix.

Before acceptance, verify full multiline source slices, adjacent-definition
exclusion, shebang/parser prefixes, Unicode, ordinary/inherent/trait/local call
ownership, duplicate/line-movement identities and strict contributor checks.
Dedicated producer-upgrade history coverage and representative retrieval remain
independent gates; source-range unit tests cannot establish their completion.

Implemented as a focused post-owner AST visitor using existing checked
`source_span` conversion. Rust producer version is now 0.15.0. All 31 extractor
tests, strict all-target/all-feature Clippy and 16 Engine unit tests pass.
Full contributor CI has not yet been rerun after this Rust change. The ongoing
Sim probe uses its earlier compiled artifact and has no Rust source inputs;
its eventual result cannot validate this Rust upgrade.

The dedicated Engine File-store fixture now models 0.14.0 identifier evidence,
indexes unchanged source with real 0.15.0 extraction after reopening, and reopens
again to compare both generations. It checks exact old/new source slices,
stable symbol IDs and call edges, version-driven re-extraction, and full
StateChronicle chain audits before/after restart. It uses the existing Penelope
Engine publication path and passes alongside strict fixture Clippy. This is a
synthetic legacy-evidence fixture, not execution of an archived 0.14.0 binary,
nor cross-backend upgrade coverage. Full contributor CI now passes in 180.43
seconds, including this fixture, all 16 backend-conformance scenarios, workspace
and doc tests, strict Clippy, architecture/migration/extension checks, dependency
policy and API documentation. Concurrent whole-Sim ingestion means the timing
is not isolated performance evidence.
