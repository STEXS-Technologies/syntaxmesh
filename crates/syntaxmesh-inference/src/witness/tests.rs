use super::*;
use crate::evaluator::tests::{fixture, selection};

#[test]
fn exact_witness_verifies_and_tampering_rejects() -> Result<(), InferenceError> {
    let (nodes, premises, rule, limits) = fixture();
    let claims = crate::evaluate(&selection(&nodes, &premises), &rule, &limits)?;
    for claim in claims {
        if claim.id
            != claim_identity(
                ClaimSelection {
                    repository: claim.repository,
                    worktree: claim.worktree,
                    generation: claim.generation,
                },
                &rule,
                claim.source,
                claim.target,
                &claim.premises,
            )?
        {
            return Err(InferenceError::InvalidInput);
        }
        let ordered = claim
            .premises
            .iter()
            .map(|reference| {
                premises
                    .iter()
                    .find(|premise| {
                        reference.fact == FactRef::Edge(premise.edge.id)
                            && reference.valid_from == premise.valid_from
                    })
                    .cloned()
                    .ok_or(InferenceError::InvalidInput)
            })
            .collect::<Result<Vec<_>, _>>()?;
        verify_witness(&claim, &rule, &ordered, 10, 2)?;
        let mut forged = claim.clone();
        forged.id = StableId::derive("forged", &[b"claim"]);
        if verify_witness(&forged, &rule, &ordered, 10, 2).is_ok() {
            return Err(InferenceError::InvalidInput);
        }
        forged = claim.clone();
        forged.derivation_class = DerivationClass::ModelInference;
        let mut disconnected = ordered.clone();
        disconnected
            .first_mut()
            .ok_or(InferenceError::InvalidInput)?
            .edge
            .source = rule.concept;
        if verify_witness(&forged, &rule, &ordered, 10, 2).is_ok()
            || verify_witness(&claim, &rule, &disconnected, 10, 2).is_ok()
            || verify_witness(&claim, &rule, &ordered, 1, 2).is_ok()
            || verify_witness(&claim, &rule, &ordered, 10, 0).is_ok()
        {
            return Err(InferenceError::InvalidInput);
        }
        let mut changed_rule = rule.clone();
        changed_rule.version = "2".to_owned();
        if verify_witness(&claim, &changed_rule, &ordered, 10, 2).is_ok() {
            return Err(InferenceError::InvalidInput);
        }
    }
    Ok(())
}

#[test]
fn legacy_claim_identity_remains_verifiable() -> Result<(), InferenceError> {
    let (nodes, premises, rule, limits) = fixture();
    let claims = crate::evaluate(&selection(&nodes, &premises), &rule, &limits)?;
    let mut legacy_claim = claims
        .first()
        .cloned()
        .ok_or(InferenceError::InvalidInput)?;
    legacy_claim.id = legacy_claim_identity_v2(
        ClaimSelection {
            repository: legacy_claim.repository,
            worktree: legacy_claim.worktree,
            generation: legacy_claim.generation,
        },
        &rule,
        legacy_claim.source,
        legacy_claim.target,
        &legacy_claim.premises,
    )?;
    let ordered = legacy_claim
        .premises
        .iter()
        .map(|reference| {
            premises
                .iter()
                .find(|premise| {
                    reference.fact == FactRef::Edge(premise.edge.id)
                        && reference.valid_from == premise.valid_from
                })
                .cloned()
                .ok_or(InferenceError::InvalidInput)
        })
        .collect::<Result<Vec<_>, _>>()?;
    verify_witness(&legacy_claim, &rule, &ordered, 10, 2)?;
    Ok(())
}
