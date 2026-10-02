ALTER TABLE syntaxmesh_schema_migrations
ADD COLUMN checksum TEXT CHECK (checksum IS NULL OR length(checksum) = 64);
