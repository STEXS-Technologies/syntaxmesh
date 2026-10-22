# ADR-0338: Daemon partial source processing option

Status: implemented.

Expose `--record-syntax-failures` on the Rust daemon, matching standalone CLI
indexing. Strict processing remains the default; duplicate flags reject. Set the
existing Engine source syntax policy before initial reconciliation, retaining
it through watch passes and resolver/verification reconfiguration. Reuse the
existing coverage facts, failed-source retry and publication workflows rather
than implement a second partial-processing path. The command does not weaken
storage verification or treat failed extraction as a successful empty file.

This is a fixed startup option, not a hot-reloaded project-config field.

Verification: daemon option tests preserve strict defaults and reject duplicates.
A real verified Turso daemon process rejects malformed Rust in strict mode,
publishes healthy sources in partial mode, then publishes a repaired file after
restart while preserving the retained partial generation. Targeted unit/process
tests and strict all-target daemon Clippy pass. A separate live process fixture
now verifies repair without restart in both polling and native-watch modes:
healthy facts remain available and the retained partial generation never acquires
the repaired function. Its HTTP reads select the accepted generation explicitly.
