CREATE TABLE IF NOT EXISTS syntaxmesh_change_events (
    generation BLOB PRIMARY KEY NOT NULL,
    event_id BLOB NOT NULL UNIQUE,
    payload BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS syntaxmesh_change_event_facts (
    generation BLOB NOT NULL,
    generation_sequence INTEGER NOT NULL,
    fact_kind INTEGER NOT NULL,
    fact_id BLOB NOT NULL,
    change_kind INTEGER NOT NULL,
    PRIMARY KEY (generation, fact_kind, fact_id)
);

CREATE INDEX IF NOT EXISTS syntaxmesh_change_event_fact_lookup_idx
    ON syntaxmesh_change_event_facts(fact_kind, fact_id, generation_sequence);

CREATE INDEX IF NOT EXISTS syntaxmesh_change_events_generation_idx
    ON syntaxmesh_change_events(generation);
