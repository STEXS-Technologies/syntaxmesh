DROP INDEX IF EXISTS syntaxmesh_fact_versions_source_idx;
DROP INDEX IF EXISTS syntaxmesh_fact_versions_target_idx;
CREATE INDEX syntaxmesh_fact_versions_source_idx
    ON syntaxmesh_fact_versions(fact_kind, source_id, valid_from_sequence, valid_until_sequence);
CREATE INDEX syntaxmesh_fact_versions_target_idx
    ON syntaxmesh_fact_versions(fact_kind, target_id, valid_from_sequence, valid_until_sequence);
