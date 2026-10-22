CREATE TABLE inference_contract_outcome_episodes (
    repository TEXT NOT NULL,
    worktree TEXT NOT NULL,
    contract_id TEXT NOT NULL,
    contract_version TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('supported', 'contradicted', 'insufficient_evidence')),
    first_generation TEXT NOT NULL,
    first_sequence INTEGER NOT NULL CHECK (first_sequence >= 0),
    last_generation TEXT NOT NULL,
    last_sequence INTEGER NOT NULL CHECK (last_sequence >= first_sequence),
    ended_generation TEXT,
    ended_sequence INTEGER CHECK (ended_sequence IS NULL OR ended_sequence >= last_sequence),
    PRIMARY KEY (repository, worktree, contract_id, contract_version, first_sequence)
);
CREATE INDEX inference_contract_outcome_episodes_as_of
    ON inference_contract_outcome_episodes(
        repository, worktree, contract_id, contract_version, first_sequence DESC
    );
