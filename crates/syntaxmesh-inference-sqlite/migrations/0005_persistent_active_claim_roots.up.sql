CREATE TABLE inference_active_pages (
    page TEXT PRIMARY KEY,
    payload BLOB NOT NULL
);
CREATE TABLE inference_stream_roots (
    stream TEXT NOT NULL,
    generation TEXT NOT NULL,
    root TEXT REFERENCES inference_active_pages(page),
    PRIMARY KEY (stream, generation),
    FOREIGN KEY (stream, generation) REFERENCES inference_stream_checkpoints(stream, generation)
);
CREATE INDEX inference_stream_claim_closed_at
    ON inference_stream_claims(stream, closed_sequence, claim);
