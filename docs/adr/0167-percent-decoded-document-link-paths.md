# ADR-0167: Percent-decoded documentation link paths

- Status: accepted
- Date: 2026-09-30

## Decision

Reuse Shardline's `sdx/src/url.rs` strict UTF-8 `percent_decode_str`
approach and the existing documentation dependency. Split literal query and
fragment delimiters first, then decode the path exactly once before existing
scheme, repository-boundary, extension, and component checks. Preserve literal
plus signs and decoded spaces. Malformed percent escapes remain literal, as
with Shardline's decoder; invalid UTF-8 and control characters are rejected.
Reject decoded backslashes, question marks, and hash characters rather than
confusing host-specific separators or the existing `path#anchor` identity.
Encoded separators and dot segments undergo the same boundary checks as raw
paths. No filesystem access, runtime, public DTO, migration, or resolver is added.

Bump documentation extractor identity from 7 to 8 so unchanged source files
are re-extracted. Exact indexed lookup and retained history remain unchanged.

## Verification

Cover spaces, Unicode, literal plus/percent signs, one-pass decoding, encoded
schemes/network paths/traversal, reserved delimiters, and invalid UTF-8. Exercise
encoded document paths through the existing File/verified-Turso heading history
scenario. These are deterministic source-link checks, not AI quality evidence.

All nine documentation extractor tests pass. Both heading-history host tests
pass with raw and space/Unicode-encoded paths, including restoration of stable
occurrence IDs and explicit StateChronicle history verification. Full
`cargo make ci` passes (117.76 seconds), including strict Clippy, architecture
boundaries, registered migrations, all-feature tests, and documentation builds.
