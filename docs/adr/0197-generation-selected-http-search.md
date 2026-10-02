# ADR-0197: Generation-selected HTTP search

## Decision

Allow the existing read-only HTTP search endpoint to accept an optional
`generation` identifier, matching node and neighborhood requests. Omission
preserves startup-pinned behavior. Use the existing generation-scoped Engine
query, validate repository/worktree membership, and label the response with
the selected generation. No separate historical search implementation is added.

Malformed identifiers return 400; missing or foreign generations return 404.
The host still fails closed with 409 after its startup pin is superseded. Live
refresh remains separate: requests and response labels must share one snapshot.

## Verification

Real TCP fixtures compare old/current search results against embedded Engine
queries after a source edit, including response generation labels and invalid
or missing generation identifiers.

Verified: all six HTTP tests pass, including real TCP history comparisons for
both supported context tokenizers. Strict HTTP Clippy, formatting, and the
architecture gate pass. Full workspace CI was not rerun after this change.
