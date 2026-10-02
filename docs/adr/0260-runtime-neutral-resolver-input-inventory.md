# ADR 0260: Runtime-neutral resolver input inventory

Status: accepted

Add a default-empty `observed_inputs` method to `ModuleResolutionProvider`.
It returns sorted/deduplicated opaque UTF-8 host path strings or a diagnostic.
These are not repository-relative graph identities. Pure contracts perform no
filesystem access. Composite providers union inventories and propagate errors.
Oxc translates its synchronized observation snapshot without lossy path encoding.
Indexer and Engine expose the installed provider inventory to their host without
exposing mutable storage or coupling the engine to filesystem metadata.

This observation contract alone does not change generation planning. Subsequent
host integration must persist inventory and fingerprints using existing durable
records, recover coverage on restart, and compare contents/existence/symlinks
before deciding a source pass is unchanged. Missing inputs are essential to
detect configuration creation; arbitrary extended config names must be retained.
Unsupported external providers keep the default empty inventory and remain
responsible for their own invalidation strategy.
