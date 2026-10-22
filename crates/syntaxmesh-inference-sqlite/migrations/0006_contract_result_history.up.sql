CREATE TABLE inference_contract_evaluation_sets (
    operation TEXT PRIMARY KEY,
    repository TEXT NOT NULL,
    worktree TEXT NOT NULL,
    generation TEXT NOT NULL,
    generation_sequence INTEGER NOT NULL CHECK (generation_sequence >= 0),
    contract_count INTEGER NOT NULL CHECK (contract_count >= 0),
    payload_digest TEXT NOT NULL,
    UNIQUE (repository, worktree, generation, operation)
);
CREATE TABLE inference_contract_evaluations (
    operation TEXT NOT NULL REFERENCES inference_contract_evaluation_sets(operation),
    repository TEXT NOT NULL,
    worktree TEXT NOT NULL,
    generation TEXT NOT NULL,
    generation_sequence INTEGER NOT NULL CHECK (generation_sequence >= 0),
    contract_id TEXT NOT NULL,
    contract_version TEXT NOT NULL,
    evaluation_id TEXT NOT NULL,
    payload BLOB NOT NULL,
    payload_digest TEXT NOT NULL,
    PRIMARY KEY (operation, contract_id, contract_version),
    FOREIGN KEY (repository, worktree, generation, operation)
        REFERENCES inference_contract_evaluation_sets(repository, worktree, generation, operation)
);
CREATE UNIQUE INDEX inference_contract_evaluations_at
    ON inference_contract_evaluations(repository, worktree, generation, contract_id, contract_version);
CREATE INDEX inference_contract_evaluations_history
    ON inference_contract_evaluations(repository, worktree, contract_id, contract_version, generation_sequence, generation);
