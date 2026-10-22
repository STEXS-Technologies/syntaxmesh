use super::*;
use crate::evaluator::tests::{fixture, selection};

#[test]
fn extension_payload_bytes_are_charged_before_graph_evaluation() -> Result<(), InferenceError> {
    let (nodes, mut premises, rule, limits) = fixture();
    premises
        .first_mut()
        .ok_or(InferenceError::InvalidInput)?
        .edge
        .extension_payload = Some(syntaxmesh_core::ExtensionPayload {
        namespace: "example".to_owned(),
        schema_version: 1,
        bytes: vec![255; limits.input_bytes],
    });
    if crate::evaluate(&selection(&nodes, &premises), &rule, &limits)
        != Err(InferenceError::BudgetExhausted)
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn exact_encoded_boundary_and_oversized_rule_fail_closed() -> Result<(), InferenceError> {
    let (nodes, premises, mut rule, mut limits) = fixture();
    let selected = selection(&nodes, &premises);
    let rules = std::slice::from_ref(&rule);
    let encoded = serde_json::to_vec(&(
        selected.repository,
        selected.worktree,
        selected.generation,
        selected.nodes,
        selected.premises,
        rules,
    ))
    .map_err(|_encoding| InferenceError::IdentityEncoding)?;
    limits.input_bytes = encoded.len();
    let expected = crate::evaluate(&selected, &rule, &limits)?;
    limits.input_bytes = encoded
        .len()
        .checked_sub(1)
        .ok_or(InferenceError::InvalidInput)?;
    if crate::evaluate(&selected, &rule, &limits) != Err(InferenceError::BudgetExhausted) {
        return Err(InferenceError::InvalidInput);
    }
    limits.input_bytes = 0;
    if crate::evaluate(&selected, &rule, &limits) != Err(InferenceError::InvalidInput) {
        return Err(InferenceError::InvalidInput);
    }
    limits.input_bytes = encoded.len();
    if crate::evaluate(&selected, &rule, &limits)? != expected {
        return Err(InferenceError::InvalidInput);
    }
    rule.id = "large".repeat(encoded.len());
    if crate::evaluate(&selected, &rule, &limits) != Err(InferenceError::BudgetExhausted) {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}
