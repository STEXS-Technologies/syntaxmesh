CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_identity_observed_idx
    ON syntaxmesh_fact_versions(fact_kind, fact_id, observed_at_unix_nanos, valid_from_sequence)
    WHERE observed_at_unix_nanos IS NOT NULL;
