CREATE TABLE inference_lifecycle (
    operation TEXT PRIMARY KEY REFERENCES inference_operations(operation),
    through_generation TEXT NOT NULL,
    through_sequence INTEGER NOT NULL CHECK (through_sequence > 0),
    last_claim_count INTEGER NOT NULL DEFAULT 0 CHECK (last_claim_count >= 0),
    last_reason_count INTEGER NOT NULL DEFAULT 0 CHECK (last_reason_count >= 0)
);
INSERT INTO inference_lifecycle(operation, through_generation, through_sequence)
    SELECT operation, generation, generation_sequence FROM inference_operations;
CREATE TABLE inference_invalidations (
    operation TEXT NOT NULL,
    claim TEXT NOT NULL,
    invalidated_generation TEXT NOT NULL,
    invalidated_sequence INTEGER NOT NULL CHECK (invalidated_sequence > 0),
    PRIMARY KEY (operation, claim),
    FOREIGN KEY (operation, claim) REFERENCES inference_claims(operation, claim)
);
CREATE TABLE inference_invalidation_reasons (
    operation TEXT NOT NULL,
    claim TEXT NOT NULL,
    fact BLOB NOT NULL,
    valid_from TEXT NOT NULL,
    closed_generation TEXT NOT NULL,
    closed_sequence INTEGER NOT NULL CHECK (closed_sequence > 0),
    PRIMARY KEY (operation, claim, fact, valid_from),
    FOREIGN KEY (operation, claim) REFERENCES inference_invalidations(operation, claim)
);
