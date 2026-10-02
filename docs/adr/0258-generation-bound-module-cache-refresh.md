# ADR 0258: Generation-bound module cache refresh

Status: accepted

Add a default no-op `refresh` capability to the runtime-neutral module provider
trait, returning a diagnostic error on failure. Composite providers forward it.
The indexer invokes it once before each generation delta preparation; failures
return before publication. Source-host no-op scans do not prepare a delta and
therefore do not clear caches unnecessarily.

The Oxc adapter reuses upstream `clear_cache`, protecting both clearing and all
resolution calls with one mutex. Import/CommonJS resolver clones share caches;
clear both under the same lock for explicitness. Poisoning fails closed. No new
cache, parser, database dependency, or runtime is added to pure contracts.
Provider version v2 invalidates planning fingerprints for unchanged sources.
This refreshes filesystem observations during new generations, not a filesystem
snapshot guarantee. Independent concurrent callers must still supply coherent
source inventories. Package/tsconfig-only changes outside inventory remain open.

Verification: targeted ECMAScript/resolver/indexer tests and strict Clippy pass.
The daemon native/polling process regression keeps configuration and caller
unchanged while adding, deleting, and restoring the imported TypeScript target.
Each transition publishes the expected resolution state; historical generations
retain their earlier state, and StateChronicle history audits after shutdown.
Full workspace CI for the production refresh and daemon regression passes in
243.58 seconds, including all 13 daemon lifecycle tests and all 16 backend
conformance scenarios. The following additional targeted tests were added after
that gate began and are not claimed covered by its earlier formatting/Clippy run.

Additional targeted checks pass: composite refresh preserves provider order and
stops on the first error; indexer refresh rejection preserves retained snapshot
and generation history before publication. Strict resolver/indexer Clippy passes.
