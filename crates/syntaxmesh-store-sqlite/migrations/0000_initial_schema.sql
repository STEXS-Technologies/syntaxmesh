CREATE TABLE IF NOT EXISTS syntaxmesh_schema (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_schema_migrations (
    version INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    applied_at_unix_seconds INTEGER,
    adopted INTEGER NOT NULL CHECK (adopted IN (0, 1)),
    checksum TEXT CHECK (checksum IS NULL OR length(checksum) = 64)
);
CREATE TABLE IF NOT EXISTS syntaxmesh_manifest (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_files (
    id BLOB PRIMARY KEY NOT NULL,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_provenance (
    id BLOB PRIMARY KEY NOT NULL,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_nodes (
    id BLOB PRIMARY KEY NOT NULL,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_edges (
    id BLOB PRIMARY KEY NOT NULL,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_records (
    record_key TEXT PRIMARY KEY NOT NULL,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_generation_history (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    generation BLOB NOT NULL UNIQUE,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_generation_acceptance (
    generation BLOB PRIMARY KEY NOT NULL,
    accepted_at_unix_nanos BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_fact_versions (
    fact_kind INTEGER NOT NULL,
    fact_id BLOB NOT NULL,
    valid_from_sequence INTEGER NOT NULL,
    valid_until_sequence INTEGER,
    observed_at_unix_nanos BLOB,
    source_id BLOB,
    target_id BLOB,
    payload BLOB NOT NULL,
    PRIMARY KEY (fact_kind, fact_id, valid_from_sequence)
);
CREATE TABLE IF NOT EXISTS syntaxmesh_graph_checkpoints (
    sequence INTEGER PRIMARY KEY NOT NULL,
    generation BLOB NOT NULL UNIQUE,
    manifest BLOB NOT NULL,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_temporal_tree_pages (
    page_id BLOB PRIMARY KEY NOT NULL,
    left_page BLOB,
    right_page BLOB,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_temporal_roots (
    sequence INTEGER PRIMARY KEY NOT NULL,
    generation BLOB NOT NULL UNIQUE,
    root_id BLOB
);
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
CREATE TABLE IF NOT EXISTS syntaxmesh_generation_lineage (
    sequence INTEGER PRIMARY KEY NOT NULL,
    generation BLOB NOT NULL UNIQUE,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_change_set_versions (
    change_set_id BLOB NOT NULL,
    valid_from_sequence INTEGER NOT NULL,
    valid_until_sequence INTEGER,
    payload BLOB NOT NULL,
    PRIMARY KEY (change_set_id, valid_from_sequence)
);
CREATE TABLE IF NOT EXISTS syntaxmesh_change_set_membership_versions (
    change_set_id BLOB NOT NULL,
    event_id BLOB NOT NULL,
    valid_from_sequence INTEGER NOT NULL,
    valid_until_sequence INTEGER,
    provenance_id BLOB NOT NULL,
    PRIMARY KEY (change_set_id, event_id, valid_from_sequence)
);
CREATE INDEX IF NOT EXISTS syntaxmesh_change_set_validity_idx
    ON syntaxmesh_change_set_versions(change_set_id, valid_from_sequence, valid_until_sequence);
CREATE INDEX IF NOT EXISTS syntaxmesh_change_set_membership_set_idx
    ON syntaxmesh_change_set_membership_versions(change_set_id, valid_from_sequence, valid_until_sequence, event_id);
CREATE INDEX IF NOT EXISTS syntaxmesh_change_set_membership_event_idx
    ON syntaxmesh_change_set_membership_versions(event_id, valid_from_sequence, valid_until_sequence, change_set_id);
