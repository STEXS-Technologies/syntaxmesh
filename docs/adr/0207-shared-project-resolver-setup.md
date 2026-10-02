# ADR-0207: Shared project resolver setup

Move CLI Node/Python provider construction and fingerprint packing into the
optional source host crate. Preserve explicit profiles, ordered Python roots,
typed errors, composite provider identity, and existing resolver/export-binding
fingerprint bytes. Return the provider and matching suffix together so hosts
configure Engine and source planning consistently. The provider trait already
requires Send + Sync; no SDK contract strengthening or duplicate resolver is
needed. Engine/core remain independent of host factory dependencies.

Verify disabled defaults and compare with previous composite identity; retain
CLI Node/Python and monorepo resolution/history fixtures. Daemon wiring remains
open.

Verified: all five source-host tests, twenty CLI host-equivalence tests, and
the mixed Node/Python monorepo fixture pass. Strict source-host/CLI Clippy,
formatting, and architecture checks pass. Full workspace CI passes after the
combined ADR-0203–0207 changes (184.18 seconds), including the independently
locked out-of-tree extension fixture after offline lockfile regeneration.
