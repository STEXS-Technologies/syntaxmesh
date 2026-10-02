# ADR-0198: Query-scoped HTTP response labels

## Decision

Derive HTTP result generation labels from the admitted Engine query, not from
a separate route-side host pin read. Add an internal labeled-query helper that
returns the selected generation and result under the existing engine lock.
Neighbor cursor validation likewise uses the query's generation inside the
lock. Neighborhood serialization uses that same query generation; context
already labels its own result through the context compiler.

This preserves today's startup-pinned behavior and admission/shutdown policy.
It establishes a single request snapshot boundary needed before live refresh;
it does not implement refresh or daemon publication.

## Verification

Retain real TCP comparisons against Engine history for default/explicit search,
node lookup, neighborhood output, context, and neighbor cursor binding.

Verified: all six HTTP tests pass, including TCP historical/default result and
cursor fixtures. Strict HTTP Clippy, formatting, and architecture checks pass.
Full workspace CI was not rerun for this internal refactor.
