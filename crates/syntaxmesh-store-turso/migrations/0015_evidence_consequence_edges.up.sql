CREATE TABLE IF NOT EXISTS syntaxmesh_generation_consequences (
    sequence INTEGER PRIMARY KEY NOT NULL,
    generation BLOB NOT NULL UNIQUE,
    payload BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS syntaxmesh_consequence_edge_versions (
    edge_id BLOB PRIMARY KEY NOT NULL,
    valid_from_sequence INTEGER NOT NULL,
    valid_until_sequence INTEGER,
    source_kind TEXT NOT NULL,
    source_id BLOB NOT NULL,
    source_fact_kind INTEGER,
    source_valid_from_generation BLOB,
    target_kind TEXT NOT NULL,
    target_id BLOB NOT NULL,
    target_fact_kind INTEGER,
    target_valid_from_generation BLOB,
    payload BLOB NOT NULL,
    CHECK (valid_until_sequence IS NULL OR valid_until_sequence > valid_from_sequence),
    CHECK ((source_kind = 'fact_version' AND source_fact_kind IS NOT NULL AND source_valid_from_generation IS NOT NULL) OR (source_kind = 'change_event' AND source_fact_kind IS NULL AND source_valid_from_generation IS NULL)),
    CHECK ((target_kind = 'fact_version' AND target_fact_kind IS NOT NULL AND target_valid_from_generation IS NOT NULL) OR (target_kind = 'change_event' AND target_fact_kind IS NULL AND target_valid_from_generation IS NULL))
);
CREATE TABLE IF NOT EXISTS syntaxmesh_consequence_evidence (
    edge_id BLOB NOT NULL,
    ordinal INTEGER NOT NULL,
    fact_kind INTEGER NOT NULL,
    fact_id BLOB NOT NULL,
    valid_from_generation BLOB NOT NULL,
    PRIMARY KEY (edge_id, ordinal),
    UNIQUE (edge_id, fact_kind, fact_id, valid_from_generation),
    FOREIGN KEY (edge_id) REFERENCES syntaxmesh_consequence_edge_versions(edge_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS syntaxmesh_consequence_parents (
    edge_id BLOB NOT NULL,
    parent_edge_id BLOB NOT NULL,
    PRIMARY KEY (edge_id, parent_edge_id),
    CHECK (edge_id <> parent_edge_id),
    FOREIGN KEY (edge_id) REFERENCES syntaxmesh_consequence_edge_versions(edge_id) ON DELETE CASCADE,
    FOREIGN KEY (parent_edge_id) REFERENCES syntaxmesh_consequence_edge_versions(edge_id)
);
CREATE INDEX IF NOT EXISTS syntaxmesh_consequence_source_idx ON syntaxmesh_consequence_edge_versions(source_kind, source_id, source_fact_kind, source_valid_from_generation, valid_from_sequence, valid_until_sequence);
CREATE INDEX IF NOT EXISTS syntaxmesh_consequence_target_idx ON syntaxmesh_consequence_edge_versions(target_kind, target_id, target_fact_kind, target_valid_from_generation, valid_from_sequence, valid_until_sequence);
CREATE INDEX IF NOT EXISTS syntaxmesh_consequence_evidence_idx ON syntaxmesh_consequence_evidence(fact_kind, fact_id, valid_from_generation, edge_id);
CREATE INDEX IF NOT EXISTS syntaxmesh_consequence_parent_idx ON syntaxmesh_consequence_parents(parent_edge_id, edge_id);
