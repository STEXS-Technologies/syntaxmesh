CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_identity_time_idx
    ON syntaxmesh_fact_versions(fact_kind, fact_id, valid_from_sequence, valid_until_sequence);
CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_time_idx
    ON syntaxmesh_fact_versions(fact_kind, valid_from_sequence, valid_until_sequence);
CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_source_idx
    ON syntaxmesh_fact_versions(source_id, valid_from_sequence, valid_until_sequence);
CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_target_idx
    ON syntaxmesh_fact_versions(target_id, valid_from_sequence, valid_until_sequence);
CREATE INDEX IF NOT EXISTS syntaxmesh_fact_versions_observed_time_idx
    ON syntaxmesh_fact_versions(observed_at_unix_nanos, fact_kind, fact_id, valid_from_sequence)
    WHERE observed_at_unix_nanos IS NOT NULL;
CREATE INDEX IF NOT EXISTS syntaxmesh_generation_acceptance_time_idx
    ON syntaxmesh_generation_acceptance(accepted_at_unix_nanos, generation);
CREATE INDEX IF NOT EXISTS syntaxmesh_change_event_fact_lookup_idx
    ON syntaxmesh_change_event_facts(fact_kind, fact_id, generation_sequence);
CREATE INDEX IF NOT EXISTS syntaxmesh_change_events_generation_idx
    ON syntaxmesh_change_events(generation);
