CREATE TABLE IF NOT EXISTS syntaxmesh_schema_migrations (
    version INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    applied_at_unix_seconds INTEGER,
    adopted INTEGER NOT NULL CHECK (adopted IN (0, 1))
);
