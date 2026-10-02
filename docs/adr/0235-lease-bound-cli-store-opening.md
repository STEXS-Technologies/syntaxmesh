# ADR 0235: Bind shared CLI store opening to the lease target

Status: accepted

## Decision

Expose the existing canonical target as `WriterLease::target() -> &Path`.
The private shared CLI Turso Engine opener accepts a borrowed `WriterLease`,
not an independent database pathname, and opens its canonical target.
Reuse the current lease for extension import; never acquire it twice.

## Limits

This prevents forgetting a lease at this shared opening boundary or supplying
an unrelated path. It does not couple Engine lifetime to the borrowed guard;
callers must still retain their guard through query and output. Filesystem
replacement by noncooperating processes remains outside advisory coordination.

## Verification

`cargo make ci` passes in 183.76 seconds, including strict workspace Clippy,
architecture checks, extension import isolation, four ownership tests, 15
attachment tests, ten daemon lifecycle tests, and 16 backend conformance
scenarios. The ownership fixture checks that the lease exposes the canonical
database target. Existing extension import and attached fallback tests continue
to pass without double acquisition.
