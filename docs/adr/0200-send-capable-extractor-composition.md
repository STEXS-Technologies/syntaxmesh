# ADR-0200: Send-capable extractor composition

## Decision

Preserve the existing local `CompositeExtractor` and add
`SendCompositeExtractor` for hosts that move an Engine across threads. Both
are concrete aliases over one generic extractor registry. Extension validation,
dispatch, producer identity, and fingerprints share the existing implementation.
Only registration differs: the Send alias requires `LanguageExtractor + Send`.

Do not add Send as a supertrait of the language SDK: local non-Send extractors
remain valid in embedded runtimes. No runtime, transport, workflow, or database
dependencies are added. This enables later generic shared HTTP hosting but does
not itself remove the HTTP Rust-extractor restriction.

## Verification

Retain the original composition suite and verify identical local/Send
fingerprints and extraction dispatch; move the Send registry to a thread.

Verified: all six SDK tests pass, including unchanged local behavior with an
Rc-backed non-Send extractor and Send-registry transfer/atomic registration.
All-feature workspace compilation, strict SDK Clippy, formatting, and
architecture checks pass. Full workspace CI was not rerun for this change.
