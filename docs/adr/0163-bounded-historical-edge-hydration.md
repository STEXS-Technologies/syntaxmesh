# ADR-0163: Bounded historical edge hydration batches

- Status: rejected after measured trial
- Date: 2026-09-30

## Trial

Borrow the Turso adapter's bounded parameterized ID-batch pattern for historical
incidence payload hydration. Fetch at most 128 requested edge IDs per SQL call,
using the existing latest-version-at-sequence and validity predicates. Retain
the persistent incidence walker and its lookahead unchanged. Hydrate only the
requested page, not its lookahead edge. Restore incidence order explicitly.

Check each SQL row's identity against the decoded payload, requested IDs, and
endpoint direction; reject duplicates and missing/inactive edges. Do not weaken
these checks by merely sorting model payload IDs. Reuse the existing blob decoder.
Keep batching internal to Turso, with no schema, DTO, query-port, or host change.
Instrumentation counts actual batch SELECTs; tree-page reads remain unchanged.

Verify tail batches, reordered results, inactive versions, and corrupt identities,
plus the real multi-page cross-backend deletion/restart fixture. Performance is
unproven until measured; this reduces hydration SELECT count, not total graph
work or a general complexity bound.

## Decision

Keep ADR-0162's prepared point reads. A 128-ID `IN` trial regressed the first dense
Turso historical context read to 14.47 seconds and was stopped after confirming
the regression. An explicit requested-ID `VALUES` join passed the complete
multi-page deletion/restart fixture and batch-count checks, but took 4.87/5.21
seconds for the initial capacity-one/two queries and 85.29 seconds overall.
The preceding prepared point-read run took 4.07/4.52 seconds and 80.62 seconds
overall. These single-run debug measurements do not establish general latency;
they do not justify replacing the simpler implementation with this batch path.

Remove the experimental hydration module and its dedicated tests. Retain a
real-adapter instrumentation regression in the existing multi-page fixture:
both directions, 1,000-edge first pages, and short tail pages must fetch exactly
the selected payload rows and count two generation/root SELECTs plus loaded
tree pages plus one payload SELECT per selected edge. Further read changes need
isolated query-plan/stage evidence rather than assuming fewer SQL calls are faster.

The retained prepared point-read implementation passes the complete
`cargo make ci` gate. The separate all-feature multi-page run passes in 79.22
seconds, exercising the new instrumented first-page and tail-page assertions.
These final verification runs overlap and are not comparative benchmarks.
