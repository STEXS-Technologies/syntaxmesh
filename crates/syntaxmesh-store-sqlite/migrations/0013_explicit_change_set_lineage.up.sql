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
