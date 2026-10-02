# ADR-0268: Shared identifier normalization

Status: accepted, 2026-10-01.

Expose the existing query identifier normalizer as `identifier_terms`, with
`IDENTIFIER_NORMALIZATION_REVISION` identifying its current behavior. Context
ranking uses the same function. Derived candidate indexes must record this
revision and rebuild when it changes; canonical substring search is unaffected.

Reuse the current camelCase, acronym, Unicode lowercase and punctuation rules,
not a second tokenizer in a database adapter. Terms are sorted and deduplicated;
no stemming, stop-word removal or semantic inference is introduced. This is a
pure query utility and adds no host/database dependency. It does not implement
the candidate index or close whole-repository relevance gates.
