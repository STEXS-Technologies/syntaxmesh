use crate::{Claim, InferenceError, Limits, Rule, Selection};
use std::collections::BTreeMap;
use syntaxmesh_core::StableId;

/// Canonical semantic configuration identity, independent of execution budgets.
///
/// # Errors
/// Rejects empty/duplicate rule identities and exhausted count/byte admission.
pub fn rule_set_identity(
    rules: &[Rule],
    rule_budget: usize,
    byte_budget: usize,
) -> Result<StableId, InferenceError> {
    if rules.is_empty() || rule_budget == 0 {
        return Err(InferenceError::InvalidInput);
    }
    if rules.len() > rule_budget {
        return Err(InferenceError::BudgetExhausted);
    }
    crate::input_budget::validate_encoded(rules, byte_budget)?;
    let mut ordered = rules.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| (&left.id, &left.version).cmp(&(&right.id, &right.version)));
    if ordered
        .iter()
        .any(|rule| rule.id.trim().is_empty() || rule.version.trim().is_empty())
        || ordered.windows(2).any(|pair| {
            pair.first()
                .zip(pair.get(1))
                .is_some_and(|(left, right)| left.id == right.id && left.version == right.version)
        })
    {
        return Err(InferenceError::InvalidInput);
    }
    let encoded =
        serde_json::to_vec(&ordered).map_err(|_encoding| InferenceError::IdentityEncoding)?;
    Ok(StableId::derive("inference-rule-set-v1", &[&encoded]))
}

/// Evaluate independent rules with shared work/output bounds and stable ordering.
///
/// # Errors
/// Rejects duplicate rules, malformed inputs, zero bounds and any exhausted limit.
pub fn evaluate_rules(
    selection: &Selection<'_>,
    rules: &[Rule],
    limits: &Limits,
    rule_budget: usize,
) -> Result<Vec<Claim>, InferenceError> {
    evaluate_rules_with_budget(selection, rules, limits, rule_budget, limits.edge_visits)
        .map(|(claims, _remaining)| claims)
}

pub(crate) fn evaluate_rules_with_budget(
    selection: &Selection<'_>,
    rules: &[Rule],
    limits: &Limits,
    rule_budget: usize,
    edge_visit_budget: usize,
) -> Result<(Vec<Claim>, usize), InferenceError> {
    let mut remaining = edge_visit_budget;
    let ordered = admit_rules(selection, rules, limits, rule_budget, &mut remaining)?;
    let mut claims = Vec::new();
    for rule in ordered {
        crate::evaluator::evaluate_into(selection, rule, limits, &mut remaining, &mut claims)?;
    }
    claims.sort_by_key(|claim| claim.id);
    Ok((claims, remaining))
}

pub(crate) fn admit_rules<'rules>(
    selection: &Selection<'_>,
    rules: &'rules [Rule],
    limits: &Limits,
    rule_budget: usize,
    remaining: &mut usize,
) -> Result<Vec<&'rules Rule>, InferenceError> {
    if rule_budget == 0 || rules.is_empty() {
        return Err(InferenceError::InvalidInput);
    }
    if rules.len() > rule_budget {
        return Err(InferenceError::BudgetExhausted);
    }
    crate::input_budget::validate(selection, rules, limits)?;
    let mut ordered = rules.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| (&left.id, &left.version).cmp(&(&right.id, &right.version)));
    if ordered.windows(2).any(|pair| {
        pair.first()
            .zip(pair.get(1))
            .is_some_and(|(left, right)| left.id == right.id && left.version == right.version)
    }) {
        return Err(InferenceError::InvalidInput);
    }
    let mut ontology_versions = BTreeMap::new();
    for rule in &ordered {
        for (namespace, version) in &rule.ontology_versions {
            *remaining = remaining
                .checked_sub(1)
                .ok_or(InferenceError::BudgetExhausted)?;
            if namespace.trim().is_empty()
                || version.trim().is_empty()
                || ontology_versions
                    .insert(namespace, version)
                    .is_some_and(|previous| previous != version)
            {
                return Err(InferenceError::InvalidInput);
            }
        }
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests;
