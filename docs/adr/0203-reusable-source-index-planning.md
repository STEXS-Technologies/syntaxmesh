# ADR-0203: Reusable source-index planning

## Decision

Move the existing CLI source marker, no-op comparison, and transition identity
policy into the runtime-neutral Engine package. Preserve marker encoding,
namespace, and generation derivation for existing stores. Expose planning over
store ports and an Engine method using its configured scope and owned store.
Hosts still supply ordered source/configuration fingerprint bytes; scanning,
filesystem roots, resolver construction, provider configuration, and output
remain host responsibilities.

Planning records intent before graph publication, exactly as the existing CLI
does. A marker is not proof of publication: reuse requires matching current
files and an accepted scoped marker generation. Callers must recover pending
workflows before planning and publish needed generations through the Engine.
This provides shared policy for daemon no-op reconciliation without duplicating
CLI history semantics. No database or runtime adapter dependencies are added.

## Verification

Retain CLI exact-repeat/edit/revert and semantic-history fixtures. Exercise
Engine planning with initial publication, unchanged repeat, edit, and revert.

Verified: the Engine planning fixture passes, including uncommitted-marker
replanning and distinct revert identity. CLI host-equivalence, invalid-record
recovery, local-command semantic lifecycle, and both watch integration fixtures
pass. Strict Engine/CLI Clippy, formatting, and architecture checks pass. Full
workspace CI was not rerun after moving this policy.
