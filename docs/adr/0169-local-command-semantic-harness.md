# ADR-0169: Local command semantic harness

- Status: accepted
- Date: 2026-09-30

## Decision

Provide an explicit host-only `--semantic-command <JSON argv array or @path>` option
alongside `--semantic <model>`. Invoke that executable directly, not through a
shell. Send the existing bounded extraction instructions and source prompt on
stdin; require one existing claims JSON object on stdout. Harness authentication,
model choice, reasoning effort, and inference belong to the configured command,
not SyntaxMesh API implementations. Argv JSON files are bounded to 64 KiB.
JSON is a bounded command result, not
NDJSON storage or an internal event-stream protocol.

Reuse the sibling orchestrator's argv/stdio Codex invocation pattern without
its JSONL event parsing, provider APIs, tool orchestration, or session handling.
Reuse SyntaxMesh's bounded packing, evidence validation, Penelope cache/retry,
atomic semantic publication, and historical graph. Command argv and prompt
identity participate in cache identity. The model label must describe the
configured command; use an asserted revision when aliases/settings change.
The host does not claim to discover an immutable remote model revision.

Commands execute serially by bounded prompt group in a temporary working
directory, with a 180-second execution limit and 4 MiB stdout/stderr limits.
Stdio uses anonymous temporary file descriptors to avoid pipe deadlock; source
inputs and captures are transient, not canonical storage. The command owns any
subprocesses it creates; host timeout terminates/reaps its direct child only.
Do not inherit indexed-project instruction/config files.
An explicitly configured external AI harness is a permitted host process;
analyzed language runtimes remain forbidden elsewhere. Core, DTO, thin runtime
protocol, and Engine stay free of process/transport dependencies. The caller
authorizes the executable and its network behavior by opting into this command;
local harness execution does not mean model inference is offline.

For the requested Codex backend, configure `codex exec` with `gpt-6-luna`,
`model_reasoning_effort="medium"`, ephemeral/read-only execution, never approvals,
and stdin input. Use saved local harness auth. No model substitution, installation,
credential reading, or Codex-specific protocol is implemented in the adapter.
Official invocation reference: https://developers.openai.com/codex/noninteractive.

## Verification

Cover argv parsing, shell-free transport, malformed/oversized output, execution
failure, caching, source evidence, and historical isolation. Test fixtures are
Rust executables; they do not prove live model quality or account availability.

The Rust subprocess fixture covers both File and verified Turso, including
invalid JSON/UTF-8, unsuccessful execution, oversized stdout and diagnostics,
model switching, offline cache reuse, sparse edits, and retained history.
The runtime audit permits dynamic executable selection only in this adapter;
literal analyzed-language runtimes remain rejected even there.

A small live Codex/GPT-6 Luna/medium fixture initially accepted four claims
from two documents in one call. An unchanged repeat launched none. After editing
one document, the first response failed `InvalidClaim` validation and was not
published; a retry invoked only that pending document, reused the other document,
and accepted five combined claims. An offline repeat launched none. The original
historical graph's canonical fact export retained SHA-256
`7b0692bad4114904cee8cab5b5c8fcb01af5b147a0d7d5d73059a1af43ab6788`.
The store passed logical integrity, root, and reference checks. This is operability
and isolation evidence, not representative extraction quality. Token usage remains
unavailable. Combined contributor checks pass (`cargo make ci`, 109.27 seconds).
