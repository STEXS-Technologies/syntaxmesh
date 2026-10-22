use crate::{Claim, ClaimSelection, DerivationClass, InferenceError, Premise, Rule};
use std::collections::BTreeSet;
use syntaxmesh_core::{FactRef, FactVersionRef, NodeId, RelationKind, StableId};

#[derive(serde::Serialize)]
struct LegacyRuleV2<'rule> {
    id: &'rule str,
    version: &'rule str,
    ontology_versions: &'rule std::collections::BTreeMap<String, String>,
    configuration_fingerprint: StableId,
    path_relation: &'rule RelationKind,
    join_relation: &'rule RelationKind,
    declaration_relation: &'rule RelationKind,
    concept: NodeId,
    output_relation: &'rule RelationKind,
}

fn legacy_claim_identity_v2(
    scope: ClaimSelection,
    rule: &Rule,
    source: NodeId,
    target: NodeId,
    premises: &[FactVersionRef],
) -> Result<StableId, InferenceError> {
    let legacy_rule = LegacyRuleV2 {
        id: &rule.id,
        version: &rule.version,
        ontology_versions: &rule.ontology_versions,
        configuration_fingerprint: rule.configuration_fingerprint,
        path_relation: &rule.path_relation,
        join_relation: &rule.join_relation,
        declaration_relation: &rule.declaration_relation,
        concept: rule.concept,
        output_relation: &rule.output_relation,
    };
    let encoded = serde_json::to_vec(&(
        2_u32,
        DerivationClass::DeterministicRule,
        scope.repository,
        scope.worktree,
        scope.generation,
        legacy_rule,
        source,
        target,
        premises,
    ))
    .map_err(|_encoding| InferenceError::IdentityEncoding)?;
    Ok(StableId::derive("inference-claim-v2", &[&encoded]))
}

pub(crate) fn claim_identity(
    scope: ClaimSelection,
    rule: &Rule,
    source: NodeId,
    target: NodeId,
    premises: &[FactVersionRef],
) -> Result<StableId, InferenceError> {
    let encoded = serde_json::to_vec(&(
        2_u32,
        DerivationClass::DeterministicRule,
        scope.repository,
        scope.worktree,
        scope.generation,
        rule,
        source,
        target,
        premises,
    ))
    .map_err(|_encoding| InferenceError::IdentityEncoding)?;
    Ok(StableId::derive("inference-claim-v3", &[&encoded]))
}

/// Verify one supported derivation; premise version authority is the caller's duty.
///
/// # Errors
/// Rejects altered metadata/identity, disconnected witnesses and exhausted bounds.
pub fn verify_witness(
    claim: &Claim,
    rule: &Rule,
    premises: &[Premise],
    premise_budget: usize,
    max_depth: usize,
) -> Result<(), InferenceError> {
    if premise_budget == 0 || max_depth == 0 {
        return Err(InferenceError::InvalidInput);
    }
    if premises.len() > premise_budget
        || claim.premises.len() > premise_budget
        || rule.ontology_versions.len() > premise_budget
    {
        return Err(InferenceError::BudgetExhausted);
    }
    if rule
        .ontology_versions
        .iter()
        .any(|(namespace, version)| namespace.trim().is_empty() || version.trim().is_empty())
    {
        return Err(InferenceError::InvalidInput);
    }
    if claim.derivation_class != DerivationClass::DeterministicRule
        || claim.rule_id != rule.id
        || claim.rule_version != rule.version
        || claim.ontology_versions != rule.ontology_versions
        || claim.configuration_fingerprint != rule.configuration_fingerprint
        || claim.relation != rule.output_relation
        || rule.id.is_empty()
        || rule.version.is_empty()
        || premises.len() != claim.premises.len()
        || !matches!(&rule.output_relation, RelationKind::External { namespace, relation } if !namespace.is_empty() && namespace != "core" && !relation.is_empty())
    {
        return Err(InferenceError::InvalidInput);
    }
    for (premise, reference) in premises.iter().zip(&claim.premises) {
        if premise.generation != claim.generation
            || premise.valid_from != reference.valid_from
            || reference.fact != FactRef::Edge(premise.edge.id)
        {
            return Err(InferenceError::InvalidInput);
        }
    }
    let (declaration, earlier) = premises.split_last().ok_or(InferenceError::InvalidInput)?;
    let (join, path) = earlier.split_last().ok_or(InferenceError::InvalidInput)?;
    if path.is_empty() {
        return Err(InferenceError::InvalidInput);
    }
    if path.len() > max_depth {
        return Err(InferenceError::DepthExhausted);
    }
    let mut current = claim.source;
    let mut visited = BTreeSet::from([current]);
    for step in path {
        if step.edge.source != current
            || step.edge.relation != rule.path_relation
            || !visited.insert(step.edge.target)
        {
            return Err(InferenceError::InvalidInput);
        }
        current = step.edge.target;
    }
    if join.edge.source != current
        || join.edge.target != claim.target
        || join.edge.relation != rule.join_relation
        || declaration.edge.source != claim.target
        || declaration.edge.target != rule.concept
        || declaration.edge.relation != rule.declaration_relation
    {
        return Err(InferenceError::InvalidInput);
    }
    let scope = ClaimSelection {
        repository: claim.repository,
        worktree: claim.worktree,
        generation: claim.generation,
    };
    let expected =
        if legacy_claim_identity_v2(scope, rule, claim.source, claim.target, &claim.premises)?
            == claim.id
        {
            claim.id
        } else {
            claim_identity(scope, rule, claim.source, claim.target, &claim.premises)?
        };
    if claim.id != expected {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
