# ADR 0256: Serialized source policy reconfiguration

Status: accepted

Add explicit mutable module-provider replacement to the indexer and a serialized
engine source-policy update (optional resolver plus verification flag). `None`
removes an existing resolver. Updates affect future preparation/publication only;
they do not publish, recover, reopen stores, or rewrite historical generations.
Exclusive `&mut` access requires hosts to serialize policy changes with queries
and publication. Providers are constructed and validated before the update.
Host planning fingerprints must be updated together with policy while holding
the same engine lock; otherwise unchanged-source planning may reuse stale facts.

This extends the existing consuming builder APIs, using the project's current
provider trait and host policy separation rather than another engine or config
parser. No database/host dependency enters pure contracts. Daemon file reload,
failure behavior, and verification-chain transitions still require integration
tests before claiming operational reload support.

Engine all-feature compilation and strict Engine/indexer Clippy pass. The
existing consuming-engine fixture additionally checks verification policy
enable/disable without changing workflow diagnostics, then transfers the same
store and confirms generation and completed-operation retention. Daemon reload
integration and provider removal are now exercised by ADR-0257's native/polling
process fixture with actual TypeScript import-edge retraction and retained
historical resolution. Positive CLI verification precedence also retains an
auditable StateChronicle chain. Full CI passes in 216.47 seconds. Verification
enable/disable transitions without the CLI override now pass in ADR-0257's
native/polling process fixture, including an auditable retained verified head
and explicit failure after an unverified gap. The new full gate passes in
171.06 seconds, including all 12 daemon process tests and all 16 backend
conformance scenarios.
