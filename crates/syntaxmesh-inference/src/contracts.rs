//! Bounded, evidence-preserving evaluation of explicit relation contracts.

use crate::{InferenceError, Selection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use syntaxmesh_core::{FactRef, FactVersionRef, GenerationId, NodeId, RelationKind, StableId};

/// Origin of a contract declaration or evidence-derived expectation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractOrigin {
    SourceDerived,
    SchemaDerived,
    ConfigDerived,
    Declared,
    Documented,
    RuntimeInferred,
    ModelSuggested,
}

/// Whether one exact relation is required or forbidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationExpectation {
    Required,
    Forbidden,
}

/// An explicit relation contract over stable graph entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub id: String,
    pub version: String,
    pub origin: ContractOrigin,
    pub source: NodeId,
    pub target: NodeId,
    pub relation: RelationKind,
    pub expectation: RelationExpectation,
}

/// Evidence-aware result. Missing relation evidence is always indeterminate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractOutcome {
    Supported,
    Contradicted,
    InsufficientEvidence,
}

/// One deterministic contract result for a selected graph generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractEvaluation {
    pub id: StableId,
    pub contract_id: String,
    pub contract_version: String,
    pub origin: ContractOrigin,
    pub generation: GenerationId,
    pub outcome: ContractOutcome,
    pub evidence: Option<FactVersionRef>,
}

/// Independent limits for contract count, encoded input, fact visits and results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractLimits {
    pub contracts: usize,
    pub input_bytes: usize,
    pub edge_visits: usize,
    pub results: usize,
}

/// Evaluate exact relation contracts without treating missing edges as proof.
///
/// Every retained premise is checked to belong to the selected generation and
/// to have unique edge identity. At most one exact evidence version is retained
/// for each result; deterministic ordering chooses the lowest edge identity.
///
/// # Errors
/// Rejects malformed/duplicate contracts and exhausted admission or work limits.
pub fn evaluate_contracts(
    selection: &Selection<'_>,
    contracts: &[Contract],
    limits: &ContractLimits,
) -> Result<Vec<ContractEvaluation>, InferenceError> {
    if limits.contracts == 0 || limits.input_bytes == 0 || limits.edge_visits == 0 {
        return Err(InferenceError::InvalidInput);
    }
    if contracts.len() > limits.contracts || contracts.len() > limits.results {
        return Err(InferenceError::BudgetExhausted);
    }
    crate::input_budget::validate_encoded(
        &(
            selection.repository,
            selection.worktree,
            selection.generation,
            selection.nodes,
            selection.premises,
            contracts,
        ),
        limits.input_bytes,
    )?;

    let mut ordered = contracts.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| (&left.id, &left.version).cmp(&(&right.id, &right.version)));
    if ordered.iter().any(|contract| {
        contract.id.trim().is_empty()
            || contract.version.trim().is_empty()
            || !selection.nodes.contains(&contract.source)
            || !selection.nodes.contains(&contract.target)
    }) || ordered.windows(2).any(|pair| {
        pair.first()
            .zip(pair.get(1))
            .is_some_and(|(left, right)| left.id == right.id && left.version == right.version)
    }) {
        return Err(InferenceError::InvalidInput);
    }

    let mut edge_ids = BTreeSet::new();
    for premise in selection.premises {
        if premise.generation != selection.generation
            || !selection.nodes.contains(&premise.edge.source)
            || !selection.nodes.contains(&premise.edge.target)
            || !edge_ids.insert(premise.edge.id)
        {
            return Err(InferenceError::InvalidInput);
        }
    }

    let mut evidence_by_relation = HashMap::new();
    if !ordered.is_empty() {
        if selection.premises.len() > limits.edge_visits {
            return Err(InferenceError::BudgetExhausted);
        }
        for premise in selection.premises {
            let relation = serde_json::to_vec(&premise.edge.relation)
                .map_err(|_encoding| InferenceError::IdentityEncoding)?;
            let key = (premise.edge.source, premise.edge.target, relation);
            let version = FactVersionRef {
                fact: FactRef::Edge(premise.edge.id),
                valid_from: premise.valid_from,
            };
            evidence_by_relation
                .entry(key)
                .and_modify(|current: &mut (syntaxmesh_core::EdgeId, FactVersionRef)| {
                    if premise.edge.id < current.0 {
                        *current = (premise.edge.id, version);
                    }
                })
                .or_insert((premise.edge.id, version));
        }
    }
    let mut results = Vec::with_capacity(ordered.len());
    for contract in ordered {
        let relation = serde_json::to_vec(&contract.relation)
            .map_err(|_encoding| InferenceError::IdentityEncoding)?;
        let evidence = evidence_by_relation
            .get(&(contract.source, contract.target, relation))
            .copied();
        let outcome = match (contract.expectation, evidence.is_some()) {
            (RelationExpectation::Required, true) => ContractOutcome::Supported,
            (RelationExpectation::Forbidden, true) => ContractOutcome::Contradicted,
            (_, false) => ContractOutcome::InsufficientEvidence,
        };
        let evidence = evidence.map(|(_edge_id, version)| version);
        let encoded = serde_json::to_vec(&(contract, selection.generation, outcome, evidence))
            .map_err(|_encoding| InferenceError::IdentityEncoding)?;
        let id = StableId::derive("semantic-contract-evaluation-v1", &[&encoded]);
        results.push(ContractEvaluation {
            id,
            contract_id: contract.id.clone(),
            contract_version: contract.version.clone(),
            origin: contract.origin,
            generation: selection.generation,
            outcome,
            evidence,
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests;
