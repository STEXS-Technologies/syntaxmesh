# ADR-0058: Invalidate indexed facts when extractor identity changes

- Status: accepted
- Date: 2026-09-28

## Context

The indexer currently skips a file when its path and content hash are unchanged.
That is incorrect after a language pack changes its parser or extraction
semantics: unchanged source would retain stale graph facts indefinitely. The
v0 plan requires parser/extractor configuration fingerprints as part of
incremental indexing. A composite router also needs to identify the selected
pack for each file before deciding whether parsing can be skipped.

## Decision

- Every `LanguageExtractor` supplies a stable producer identity consisting of
  its namespace and version. Composite extractors route identity lookup by the
  same normalized extension map used for parsing.
- Extractor provenance must include the exact source file ID and content hash;
  index publication rejects outputs that omit or mismatch that source binding.
- Before skipping an unchanged file, the indexer compares its current
  provenance identity with the selected extractor's identity. Missing or
  changed identity invalidates that file's owned facts and re-extracts it.
- The selected extractor set's stable configuration fingerprint is included
  in host-derived generation identities. Composite fingerprints cover sorted
  extension-to-producer mappings, so changing the registered pack or version
  changes the host's generation ID even when source bytes are unchanged.
- Extractor provenance IDs continue to bind namespace to file content; the
  same provenance identity is upserted with the new version on re-extraction.
  Fact identity remains a separate concern and continues to be controlled by
  language-pack stable-ID rules.

## Alternatives considered

- Reparse every file on every run: rejected because it defeats incremental
  indexing and fails the zero-parser-work unchanged-scan gate.
- Treat source hashes as including parser version: rejected because file
  content identity must describe bytes, not tool configuration.
- Rely on each host to force a new generation manually: rejected because it
  does not identify which files need reparsing and is easy to get wrong.
- Persist one global extractor-set fingerprint in the graph manifest:
  deferred; per-file provenance already records the producer identity needed
  for precise invalidation, and no store schema change is required.

## Consequences

- Custom extractors must provide stable producer namespace/version identity.
- A language-pack version bump re-extracts only files routed to that pack;
  unchanged files from other packs remain parser-cache hits.
- Tests must prove no extraction for unchanged bytes with unchanged identity,
  re-extraction for unchanged bytes after identity change, and stable composite
  configuration fingerprints independent of registration order.
- All hosts that derive generation IDs from a source scan must include the
  extractor configuration fingerprint. Engine callers that choose generation
  IDs remain responsible for that host-level contract.
