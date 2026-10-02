CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_start_generation_idx
    ON syntaxmesh_fact_versions(valid_from_sequence, fact_kind, fact_id);
CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_end_generation_idx
    ON syntaxmesh_fact_versions(valid_until_sequence, fact_kind, fact_id);
