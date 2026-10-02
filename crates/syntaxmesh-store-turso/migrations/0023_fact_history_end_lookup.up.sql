CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_identity_end_idx
    ON syntaxmesh_fact_versions(fact_kind, fact_id, valid_until_sequence, valid_from_sequence);
