CREATE TABLE inference_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    checksum TEXT NOT NULL
);
CREATE TABLE inference_operations (
    operation TEXT PRIMARY KEY,
    repository TEXT NOT NULL,
    worktree TEXT NOT NULL,
    generation TEXT NOT NULL,
    generation_sequence INTEGER NOT NULL CHECK (generation_sequence > 0),
    configuration TEXT NOT NULL,
    claim_count INTEGER NOT NULL CHECK (claim_count >= 0)
);
CREATE TABLE inference_claims (
    operation TEXT NOT NULL REFERENCES inference_operations(operation),
    claim TEXT NOT NULL,
    payload BLOB NOT NULL,
    payload_digest TEXT NOT NULL,
    PRIMARY KEY (operation, claim)
);
CREATE TABLE inference_premises (
    operation TEXT NOT NULL,
    claim TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    fact BLOB NOT NULL,
    valid_from TEXT NOT NULL,
    PRIMARY KEY (operation, claim, ordinal),
    FOREIGN KEY (operation, claim) REFERENCES inference_claims(operation, claim)
);
CREATE INDEX inference_premise_postings
    ON inference_premises(operation, fact, valid_from, claim);
CREATE TABLE inference_watermarks (
    repository TEXT NOT NULL,
    worktree TEXT NOT NULL,
    configuration TEXT NOT NULL,
    operation TEXT NOT NULL REFERENCES inference_operations(operation),
    generation_sequence INTEGER NOT NULL CHECK (generation_sequence > 0),
    PRIMARY KEY (repository, worktree, configuration)
);
