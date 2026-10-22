CREATE TABLE inference_lifecycle_receipts (
    operation TEXT NOT NULL REFERENCES inference_operations(operation),
    through_generation TEXT NOT NULL,
    through_sequence INTEGER NOT NULL CHECK (through_sequence > 0),
    affected_claims INTEGER NOT NULL CHECK (affected_claims >= 0),
    closure_reasons INTEGER NOT NULL CHECK (closure_reasons >= 0),
    PRIMARY KEY (operation, through_generation),
    UNIQUE (operation, through_sequence)
);
INSERT INTO inference_lifecycle_receipts
    SELECT operation, through_generation, through_sequence, last_claim_count, last_reason_count
    FROM inference_lifecycle;
