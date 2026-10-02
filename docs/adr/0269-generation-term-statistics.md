# ADR-0269: Generation term statistics

Status: accepted, 2026-10-01.

Add a pure query utility collecting normalized term document frequencies through
the existing historical-node page port. Pin one generation, accept at most 16
normalized terms and an explicit node scan budget. Return scanned population,
per-term counts and a completion flag: incomplete counts are lower bounds, never
exact corpus statistics. Reject nonadvancing pages. Reuse normalization v1.

Durable adapters already implement indexed pages; reference adapters may
materialize snapshots internally. The collector bounds its own retained nodes,
not every store implementation's memory. This is a building block for derived
ranking indexes, not permission to scan whole history on every context request.
