CREATE INDEX inference_contract_violation_episodes_history
    ON inference_contract_outcome_episodes (
        repository, worktree, contract_id, contract_version, first_sequence
    )
    WHERE outcome = 'contradicted';
