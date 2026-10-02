# ADR 0262: Host module input fingerprints

Status: accepted

The source host fingerprints sorted/deduplicated absolute observed paths with a
versioned, length-delimited encoding. Record missing/file/directory/other kind;
stream regular file contents into BLAKE3, without relying on modification time.
Record symlink targets separately and then fingerprint the followed object.
Non-UTF-8 link targets, relative paths, and filesystem errors other than NotFound
fail closed. Missing paths remain explicit so creation invalidates the digest.
Directory timestamps are excluded: exact child probes carry lookup existence.
Outside-root absolute inputs are included because Oxc may consult ancestor
configs and packages. Host policy must authorize its configured resolver/root.

Reuse BLAKE3 and standard filesystem access; do not parse configs or store them
as source-language facts. This is not an atomic filesystem snapshot. Integration
must compare before no-op planning and avoid acknowledging observations taken
after resolution as if they were the inputs used to produce a generation.

The fingerprint also includes the canonical path of every existing followed
object. This detects intermediate directory-symlink retargeting even when the
new target has identical bytes. Use fingerprint namespace v2 for this addition.

Verification: content/existence/order tests and Unix direct/intermediate symlink
retarget/broken/missing tests pass, along with strict source-host Clippy. These
are host fingerprint primitives, not evidence that no-op planning is integrated.
