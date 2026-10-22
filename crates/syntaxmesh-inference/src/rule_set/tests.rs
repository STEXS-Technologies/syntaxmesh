use super::*;
use crate::evaluator::tests::{fixture, selection};

#[test]
fn configuration_identity_is_order_independent_and_binds_rule_metadata()
-> Result<(), InferenceError> {
    let (_nodes, _premises, rule, _limits) = fixture();
    let mut second = rule.clone();
    second.id = "example:second".to_owned();
    let mut rules = vec![rule.clone(), second];
    let original = rule_set_identity(&rules, 2, 16_384)?;
    rules.reverse();
    if rule_set_identity(&rules, 3, 32_768)? != original {
        return Err(InferenceError::InvalidInput);
    }
    for case in 0..3 {
        let mut altered = rules.clone();
        let configured = altered.first_mut().ok_or(InferenceError::InvalidInput)?;
        match case {
            0 => {
                configured
                    .ontology_versions
                    .insert("example".to_owned(), "2".to_owned());
            }
            1 => configured.configuration_fingerprint = StableId::derive("config", &[b"changed"]),
            _ => {
                configured.output_relation = syntaxmesh_core::RelationKind::External {
                    namespace: "example".to_owned(),
                    relation: "different_output".to_owned(),
                }
            }
        }
        if rule_set_identity(&altered, 2, 16_384)? == original {
            return Err(InferenceError::InvalidInput);
        }
    }
    rules
        .first_mut()
        .ok_or(InferenceError::InvalidInput)?
        .version = "2".to_owned();
    if rule_set_identity(&rules, 2, 16_384)? == original
        || rule_set_identity(&rules, 1, 16_384).is_ok()
        || rule_set_identity(&rules, 2, 1).is_ok()
        || rule_set_identity(&[rule.clone(), rule], 2, 16_384).is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn ontology_bindings_reject_conflicts_empty_identifiers_and_exhausted_validation()
-> Result<(), InferenceError> {
    let (nodes, premises, rule, mut limits) = fixture();
    let selected = selection(&nodes, &premises);
    let mut other = rule.clone();
    other.id = "example:other".to_owned();
    other
        .ontology_versions
        .insert("example".to_owned(), "2".to_owned());
    limits.claims = 4;
    if evaluate_rules(&selected, &[rule.clone(), other.clone()], &limits, 2)
        != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    other.ontology_versions =
        std::collections::BTreeMap::from([("other".to_owned(), "2".to_owned())]);
    if evaluate_rules(&selected, &[rule.clone(), other], &limits, 2)?.len() != 4 {
        return Err(InferenceError::InvalidInput);
    }
    for (namespace, version) in [("", "1"), ("example", " ")] {
        let mut invalid = rule.clone();
        invalid.ontology_versions =
            std::collections::BTreeMap::from([(namespace.to_owned(), version.to_owned())]);
        if crate::evaluate(&selected, &invalid, &limits) != Err(InferenceError::InvalidInput) {
            return Err(InferenceError::InvalidInput);
        }
    }
    let mut many = rule;
    many.ontology_versions = std::collections::BTreeMap::from([
        ("a".to_owned(), "1".to_owned()),
        ("b".to_owned(), "1".to_owned()),
    ]);
    limits.edge_visits = 1;
    if crate::evaluate(&selected, &many, &limits) != Err(InferenceError::BudgetExhausted) {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn rules_share_budgets_and_preserve_single_rule_identity() -> Result<(), InferenceError> {
    let (nodes, premises, rule, mut limits) = fixture();
    let selected = selection(&nodes, &premises);
    let single = crate::evaluate(&selected, &rule, &limits)?;
    if evaluate_rules(&selected, std::slice::from_ref(&rule), &limits, 1)? != single {
        return Err(InferenceError::InvalidInput);
    }
    let mut second = rule.clone();
    second.id = "example:second".to_owned();
    let mut rules = vec![rule.clone(), second];
    limits.claims = 4;
    let complete = evaluate_rules(&selected, &rules, &limits, 2)?;
    rules.reverse();
    if complete.len() != 4 || evaluate_rules(&selected, &rules, &limits, 2)? != complete {
        return Err(InferenceError::InvalidInput);
    }
    let mut remaining = limits.edge_visits;
    let mut counted = Vec::new();
    crate::evaluator::evaluate_into(&selected, &rule, &limits, &mut remaining, &mut counted)?;
    let used = limits
        .edge_visits
        .checked_sub(remaining)
        .ok_or(InferenceError::InvalidInput)?;
    limits.edge_visits = used
        .checked_mul(2)
        .and_then(|count| count.checked_sub(1))
        .ok_or(InferenceError::InvalidInput)?;
    if evaluate_rules(&selected, &rules, &limits, 2) != Err(InferenceError::BudgetExhausted) {
        return Err(InferenceError::InvalidInput);
    }
    limits.edge_visits = 100;
    limits.claims = 3;
    if evaluate_rules(&selected, &rules, &limits, 2) != Err(InferenceError::BudgetExhausted)
        || evaluate_rules(&selected, &rules, &limits, 1) != Err(InferenceError::BudgetExhausted)
        || evaluate_rules(&selected, &[rule.clone(), rule], &limits, 2)
            != Err(InferenceError::InvalidInput)
        || evaluate_rules(&selected, &[], &limits, 2) != Err(InferenceError::InvalidInput)
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}
