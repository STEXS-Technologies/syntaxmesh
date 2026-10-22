use super::*;
use crate::evaluator::tests::{fixture, selection};

fn contract(
    id: &str,
    source: NodeId,
    target: NodeId,
    expectation: RelationExpectation,
) -> Contract {
    Contract {
        id: id.to_owned(),
        version: "1".to_owned(),
        origin: ContractOrigin::Declared,
        source,
        target,
        relation: RelationKind::Calls,
        expectation,
    }
}

fn limits() -> ContractLimits {
    ContractLimits {
        contracts: 4,
        input_bytes: 16_384,
        edge_visits: 100,
        results: 4,
    }
}

#[test]
fn exact_evidence_supports_or_contradicts_and_absence_stays_indeterminate()
-> Result<(), InferenceError> {
    let (nodes, premises, _rule, _) = fixture();
    let edge = &premises.first().ok_or(InferenceError::InvalidInput)?.edge;
    let required = contract(
        "required",
        edge.source,
        edge.target,
        RelationExpectation::Required,
    );
    let forbidden = contract(
        "forbidden",
        edge.source,
        edge.target,
        RelationExpectation::Forbidden,
    );
    let absent = contract(
        "absent",
        edge.target,
        edge.source,
        RelationExpectation::Forbidden,
    );
    let evaluated = evaluate_contracts(
        &selection(&nodes, &premises),
        &[absent, forbidden, required.clone()],
        &limits(),
    )?;
    let mut results = evaluated.iter();
    let absent_result = results.next().ok_or(InferenceError::InvalidInput)?;
    let forbidden_result = results.next().ok_or(InferenceError::InvalidInput)?;
    let required_result = results.next().ok_or(InferenceError::InvalidInput)?;
    if results.next().is_some()
        || absent_result.contract_id != "absent"
        || absent_result.outcome != ContractOutcome::InsufficientEvidence
        || absent_result.evidence.is_some()
        || forbidden_result.outcome != ContractOutcome::Contradicted
        || forbidden_result.evidence.is_none()
        || required_result.outcome != ContractOutcome::Supported
        || required_result.evidence.is_none()
    {
        return Err(InferenceError::InvalidInput);
    }
    let repeated = evaluate_contracts(&selection(&nodes, &premises), &[required], &limits())?;
    if required_result.id != repeated.first().ok_or(InferenceError::InvalidInput)?.id {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn exact_result_retains_generation_and_opening_fact_version() -> Result<(), InferenceError> {
    let (nodes, premises, _rule, _) = fixture();
    let premise = premises.first().ok_or(InferenceError::InvalidInput)?;
    let requested = contract(
        "required",
        premise.edge.source,
        premise.edge.target,
        RelationExpectation::Required,
    );
    let evaluated = evaluate_contracts(&selection(&nodes, &premises), &[requested], &limits())?;
    let result = evaluated.first().ok_or(InferenceError::InvalidInput)?;
    if result.generation != premise.generation
        || result.evidence
            != Some(FactVersionRef {
                fact: FactRef::Edge(premise.edge.id),
                valid_from: premise.valid_from,
            })
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn malformed_duplicate_and_exhausted_contract_batches_are_rejected() -> Result<(), InferenceError> {
    let (nodes, premises, _rule, _) = fixture();
    let first = *nodes.first().ok_or(InferenceError::InvalidInput)?;
    let second = *nodes.iter().nth(1).ok_or(InferenceError::InvalidInput)?;
    let item = contract("same", first, second, RelationExpectation::Required);
    let selected = selection(&nodes, &premises);
    if evaluate_contracts(&selected, &[item.clone(), item.clone()], &limits())
        != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    if evaluate_contracts(
        &selected,
        std::slice::from_ref(&item),
        &ContractLimits {
            edge_visits: 3,
            ..limits()
        },
    ) != Err(InferenceError::BudgetExhausted)
    {
        return Err(InferenceError::InvalidInput);
    }
    if evaluate_contracts(
        &selected,
        std::slice::from_ref(&item),
        &ContractLimits {
            input_bytes: 1,
            ..limits()
        },
    ) != Err(InferenceError::BudgetExhausted)
    {
        return Err(InferenceError::InvalidInput);
    }
    if evaluate_contracts(
        &selected,
        std::slice::from_ref(&item),
        &ContractLimits {
            results: 0,
            ..limits()
        },
    ) != Err(InferenceError::BudgetExhausted)
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn mixed_generation_and_missing_contract_entities_are_rejected() -> Result<(), InferenceError> {
    let (nodes, mut premises, _rule, _) = fixture();
    let selected = selection(&nodes, &premises);
    let missing = NodeId::derive(&[b"outside-selection"]);
    let bad_entity = contract(
        "bad",
        missing,
        *nodes.first().ok_or(InferenceError::InvalidInput)?,
        RelationExpectation::Required,
    );
    if evaluate_contracts(&selected, &[bad_entity], &limits()) != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    let premise = premises.first_mut().ok_or(InferenceError::InvalidInput)?;
    premise.generation = GenerationId::derive(&[b"foreign-generation"]);
    if evaluate_contracts(&selection(&nodes, &premises), &[], &limits())
        != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}
