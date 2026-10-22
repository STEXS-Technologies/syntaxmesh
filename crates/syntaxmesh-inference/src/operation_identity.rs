use crate::{ClaimSelection, InferenceError, Rule, rule_set_identity};
use syntaxmesh_core::StableId;

/// Semantic identity of a scoped, generation-pinned complete materialization.
/// Execution budgets and storage destinations are deliberately not semantic inputs.
///
/// # Errors
/// Rejects malformed rules or exhausted rule-count/encoded-byte admission.
pub fn materialization_identity(
    selection: ClaimSelection,
    rules: &[Rule],
    rule_budget: usize,
    byte_budget: usize,
) -> Result<StableId, InferenceError> {
    let configuration = rule_set_identity(rules, rule_budget, byte_budget)?;
    let encoded = serde_json::to_vec(&(
        selection.repository,
        selection.worktree,
        selection.generation,
        configuration,
    ))
    .map_err(|_encoding| InferenceError::IdentityEncoding)?;
    Ok(StableId::derive(
        "inference-materialization-v1",
        &[&encoded],
    ))
}

#[cfg(test)]
mod tests;
