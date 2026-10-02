# Contributing

Follow the architecture rules in `AGENTS.md`, the current scope in `README.md`,
and `docs/V0_ARCHITECTURE_PLAN.md`. Record architectural decisions before changing
public contracts. Run `cargo make ci` before submitting changes.

## Git commits

Benchmark inputs must come only from STEXS-Technologies repositories. Verify
the origin before running an external corpus; do not publish private corpus
names, source paths, copied samples, fingerprints or benchmark evidence.

Use Shardline's commit conventions: brief, atomic commits with one logical
change each. Put detailed rationale, scope, testing and impact in the pull
request description.

Use `<type>: <subject>` or `<type>(<scope>): <subject>`.
Supported types are `docs`, `feat`, `fix`, `refactor`, `test`, `chore` and `perf`.
Use an imperative subject with a lowercase start (except proper nouns), no
trailing period, and aim for a title of at most 50 characters.

Examples:

- `feat(indexer): record source processing failures`
- `fix(storage): preserve history after failed writes`
- `test(engine): cover syntax failure recovery`
- `docs(architecture): define processing coverage`

Keep unrelated changes separate. Do not rewrite published history unless
explicitly requested; use an exact `--force-with-lease` guard when it is.
