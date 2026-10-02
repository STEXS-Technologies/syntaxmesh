CREATE TABLE IF NOT EXISTS syntaxmesh_temporal_incidence_roots (
    sequence INTEGER PRIMARY KEY NOT NULL,
    generation BLOB NOT NULL UNIQUE,
    root_id BLOB
);
