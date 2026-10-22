CREATE TABLE inference_streams (
    stream TEXT PRIMARY KEY,
    origin_operation TEXT NOT NULL UNIQUE REFERENCES inference_operations(operation),
    head_generation TEXT NOT NULL,
    head_sequence INTEGER NOT NULL CHECK (head_sequence > 0)
);
CREATE TABLE inference_stream_checkpoints (
    stream TEXT NOT NULL REFERENCES inference_streams(stream),
    generation TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    previous_generation TEXT,
    added_claims INTEGER NOT NULL CHECK (added_claims >= 0),
    affected_claims INTEGER NOT NULL CHECK (affected_claims >= 0),
    closure_reasons INTEGER NOT NULL CHECK (closure_reasons >= 0),
    active_claims INTEGER NOT NULL CHECK (active_claims >= 0),
    PRIMARY KEY (stream, generation),
    UNIQUE (stream, sequence)
);
CREATE TABLE inference_stream_claims (
    stream TEXT NOT NULL REFERENCES inference_streams(stream),
    claim TEXT NOT NULL,
    payload BLOB NOT NULL,
    payload_digest TEXT NOT NULL,
    born_generation TEXT NOT NULL,
    born_sequence INTEGER NOT NULL CHECK (born_sequence > 0),
    closed_generation TEXT,
    closed_sequence INTEGER,
    PRIMARY KEY (stream, claim),
    CHECK ((closed_generation IS NULL) = (closed_sequence IS NULL)),
    CHECK (closed_sequence IS NULL OR closed_sequence > born_sequence)
);
CREATE INDEX inference_stream_claim_intervals
    ON inference_stream_claims(stream, born_sequence, closed_sequence, claim);
CREATE TABLE inference_stream_premises (
    stream TEXT NOT NULL,
    claim TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    fact BLOB NOT NULL,
    valid_from TEXT NOT NULL,
    PRIMARY KEY (stream, claim, ordinal),
    FOREIGN KEY (stream, claim) REFERENCES inference_stream_claims(stream, claim)
);
CREATE INDEX inference_stream_premise_postings
    ON inference_stream_premises(stream, fact, valid_from, claim);
CREATE TABLE inference_stream_closures (
    stream TEXT NOT NULL,
    claim TEXT NOT NULL,
    fact BLOB NOT NULL,
    valid_from TEXT NOT NULL,
    closed_generation TEXT NOT NULL,
    closed_sequence INTEGER NOT NULL CHECK (closed_sequence > 0),
    PRIMARY KEY (stream, claim, fact, valid_from),
    FOREIGN KEY (stream, claim) REFERENCES inference_stream_claims(stream, claim)
);
