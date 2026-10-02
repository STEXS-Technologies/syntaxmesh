ALTER TABLE syntaxmesh_nodes
    ADD COLUMN node_kind INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS syntaxmesh_nodes_kind_idx
    ON syntaxmesh_nodes(node_kind, id);
