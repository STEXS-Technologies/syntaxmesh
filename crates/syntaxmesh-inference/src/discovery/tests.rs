use super::*;
use crate::evaluator::tests::{fixture, selection};
use syntaxmesh_core::{EdgeId, GenerationId};

fn seed(premise: &Premise) -> FactVersionRef {
    FactVersionRef {
        fact: FactRef::Edge(premise.edge.id),
        valid_from: premise.valid_from,
    }
}

fn oracle(
    selected: &Selection<'_>,
    rules: &[Rule],
    seeds: &[FactVersionRef],
    limits: &Limits,
) -> Result<Vec<Claim>, InferenceError> {
    Ok(crate::evaluate_rules(selected, rules, limits, rules.len())?
        .into_iter()
        .filter(|claim| claim.premises.iter().any(|premise| seeds.contains(premise)))
        .collect())
}

#[test]
fn every_witness_position_and_multiseed_matches_complete_oracle() -> Result<(), InferenceError> {
    let (nodes, mut premises, rule, mut limits) = fixture();
    limits.edge_visits = 1000;
    let rules = [rule];
    for premise in &premises {
        let seeds = [seed(premise)];
        let selected = selection(&nodes, &premises);
        if discover_rules(&selected, &rules, &seeds, &limits, 1)?
            != oracle(&selected, &rules, &seeds, &limits)?
        {
            return Err(InferenceError::InvalidInput);
        }
    }
    let seeds = premises.iter().map(seed).collect::<Vec<_>>();
    let original = discover_rules(&selection(&nodes, &premises), &rules, &seeds, &limits, 1)?;
    if original != oracle(&selection(&nodes, &premises), &rules, &seeds, &limits)? {
        return Err(InferenceError::InvalidInput);
    }
    premises.reverse();
    let mut reversed = seeds;
    reversed.reverse();
    if discover_rules(&selection(&nodes, &premises), &rules, &reversed, &limits, 1)? != original {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn stale_duplicate_foreign_and_empty_seeds_preserve_admission() -> Result<(), InferenceError> {
    let (nodes, mut premises, rule, limits) = fixture();
    let active = seed(premises.first().ok_or(InferenceError::InvalidInput)?);
    for seeds in [
        vec![active, active],
        vec![FactVersionRef {
            valid_from: GenerationId::derive(&[b"old"]),
            ..active
        }],
        vec![FactVersionRef {
            fact: FactRef::Node(rule.concept),
            ..active
        }],
    ] {
        if discover_rules(
            &selection(&nodes, &premises),
            std::slice::from_ref(&rule),
            &seeds,
            &limits,
            1,
        )
        .is_ok()
        {
            return Err(InferenceError::InvalidInput);
        }
    }
    if !discover_rules(
        &selection(&nodes, &premises),
        std::slice::from_ref(&rule),
        &[],
        &limits,
        1,
    )?
    .is_empty()
    {
        return Err(InferenceError::InvalidInput);
    }
    let mut invalid_rule = rule.clone();
    invalid_rule.output_relation = syntaxmesh_core::RelationKind::Calls;
    if discover_rules(
        &selection(&nodes, &premises),
        std::slice::from_ref(&invalid_rule),
        &[],
        &limits,
        1,
    )
    .is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    let first = premises.first_mut().ok_or(InferenceError::InvalidInput)?;
    first.generation = GenerationId::derive(&[b"foreign"]);
    if discover_rules(&selection(&nodes, &premises), &[rule], &[], &limits, 1).is_ok() {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn restored_versions_cycles_and_rule_order_match_oracle() -> Result<(), InferenceError> {
    let (nodes, mut premises, rule, mut limits) = fixture();
    let previous = seed(premises.first().ok_or(InferenceError::InvalidInput)?);
    premises
        .first_mut()
        .ok_or(InferenceError::InvalidInput)?
        .valid_from = GenerationId::derive(&[b"restored"]);
    let mut cycle = premises
        .first()
        .cloned()
        .ok_or(InferenceError::InvalidInput)?;
    cycle.edge.id = EdgeId::derive(&[b"discovery-cycle"]);
    cycle.edge.source = NodeId::derive(&[b"writer"]);
    cycle.edge.target = NodeId::derive(&[b"entry"]);
    premises.push(cycle);
    limits.facts = 10;
    limits.claims = 4;
    limits.edge_visits = 1000;
    let mut other = rule.clone();
    other.id = "example:other".to_owned();
    let mut rules = vec![rule, other];
    let selected = selection(&nodes, &premises);
    if discover_rules(&selected, &rules, &[previous], &limits, 2).is_ok() {
        return Err(InferenceError::InvalidInput);
    }
    let seeds = [seed(premises.first().ok_or(InferenceError::InvalidInput)?)];
    let expected = oracle(&selected, &rules, &seeds, &limits)?;
    if discover_rules(&selected, &rules, &seeds, &limits, 2)? != expected {
        return Err(InferenceError::InvalidInput);
    }
    rules.reverse();
    if discover_rules(&selected, &rules, &seeds, &limits, 2)? != expected {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn exhausted_controls_never_return_partial_discovery() -> Result<(), InferenceError> {
    let (nodes, premises, rule, limits) = fixture();
    let seeds = [seed(premises.last().ok_or(InferenceError::InvalidInput)?)];
    for control in 0..6 {
        let mut bounded = limits;
        let mut rule_budget = 1;
        match control {
            0 => bounded.input_bytes = 1,
            1 => bounded.facts = 1,
            2 => bounded.edge_visits = 1,
            3 => bounded.depth = 1,
            4 => bounded.claims = 1,
            _ => rule_budget = 0,
        }
        if discover_rules(
            &selection(&nodes, &premises),
            std::slice::from_ref(&rule),
            &seeds,
            &bounded,
            rule_budget,
        )
        .is_ok()
        {
            return Err(InferenceError::InvalidInput);
        }
    }
    Ok(())
}

#[test]
fn overlapping_relations_match_every_three_node_directed_graph() -> Result<(), InferenceError> {
    let (_nodes, templates, mut rule, mut limits) = fixture();
    let ids = [
        NodeId::derive(&[b"a"]),
        NodeId::derive(&[b"b"]),
        rule.concept,
    ];
    let nodes = ids.into_iter().collect::<BTreeSet<_>>();
    let pairs = ids
        .iter()
        .flat_map(|source| ids.iter().map(move |target| (*source, *target)))
        .collect::<Vec<_>>();
    let template = templates.first().ok_or(InferenceError::InvalidInput)?;
    rule.join_relation = rule.path_relation.clone();
    rule.declaration_relation = rule.path_relation.clone();
    limits.input_bytes = 65_536;
    limits.facts = 12;
    limits.depth = 3;
    limits.claims = 256;
    limits.edge_visits = 100_000;
    for mask in 0_u16..512 {
        let mut premises = Vec::new();
        for (index, (source, target)) in pairs.iter().enumerate() {
            let bit = 1_u16
                .checked_shl(u32::try_from(index).map_err(|_index| InferenceError::InvalidInput)?)
                .ok_or(InferenceError::InvalidInput)?;
            if mask & bit == 0 {
                continue;
            }
            let mut premise = template.clone();
            premise.edge.id = EdgeId::derive(&[b"small-graph", index.to_string().as_bytes()]);
            premise.edge.source = *source;
            premise.edge.target = *target;
            premises.push(premise);
        }
        let selected = selection(&nodes, &premises);
        let complete = crate::evaluate(&selected, &rule, &limits)?;
        for premise in &premises {
            let exact = seed(premise);
            let expected = complete
                .iter()
                .filter(|claim| claim.premises.contains(&exact))
                .cloned()
                .collect::<Vec<_>>();
            if discover_rules(&selected, std::slice::from_ref(&rule), &[exact], &limits, 1)?
                != expected
            {
                return Err(InferenceError::InvalidInput);
            }
        }
    }
    Ok(())
}

#[test]
fn unrelated_component_is_not_forward_traversed_and_irrelevant_seed_is_empty()
-> Result<(), InferenceError> {
    let (mut nodes, mut premises, rule, mut limits) = fixture();
    let exact = seed(premises.last().ok_or(InferenceError::InvalidInput)?);
    let expected = oracle(
        &selection(&nodes, &premises),
        std::slice::from_ref(&rule),
        &[exact],
        &limits,
    )?;
    let template = premises
        .first()
        .cloned()
        .ok_or(InferenceError::InvalidInput)?;
    for index in 0_usize..30 {
        let mut premise = template.clone();
        premise.edge.id = EdgeId::derive(&[b"unrelated", index.to_string().as_bytes()]);
        premise.edge.source = NodeId::derive(&[b"unrelated-node", index.to_string().as_bytes()]);
        premise.edge.target = NodeId::derive(&[
            b"unrelated-node",
            index
                .checked_add(1)
                .ok_or(InferenceError::InvalidInput)?
                .to_string()
                .as_bytes(),
        ]);
        nodes.extend([premise.edge.source, premise.edge.target]);
        premises.push(premise);
    }
    limits.input_bytes = 65_536;
    limits.facts = 100;
    limits.edge_visits = 1000;
    let selected = selection(&nodes, &premises);
    if crate::evaluate(&selected, &rule, &limits) != Err(InferenceError::DepthExhausted)
        || discover_rules(&selected, std::slice::from_ref(&rule), &[exact], &limits, 1)? != expected
    {
        return Err(InferenceError::InvalidInput);
    }
    let mut irrelevant_rule = rule;
    irrelevant_rule.path_relation = syntaxmesh_core::RelationKind::External {
        namespace: "example".to_owned(),
        relation: "other_path".to_owned(),
    };
    let irrelevant = seed(premises.first().ok_or(InferenceError::InvalidInput)?);
    if !discover_rules(&selected, &[irrelevant_rule], &[irrelevant], &limits, 1)?.is_empty() {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn exact_input_boundary_includes_seeds_and_empty_seed_zero_limits_reject()
-> Result<(), InferenceError> {
    let (nodes, premises, rule, mut limits) = fixture();
    let selected = selection(&nodes, &premises);
    let rules = [rule];
    let seeds = [seed(premises.last().ok_or(InferenceError::InvalidInput)?)];
    let bytes = serde_json::to_vec(&(
        selected.repository,
        selected.worktree,
        selected.generation,
        selected.nodes,
        selected.premises,
        &rules,
        &seeds,
    ))
    .map_err(|_encoding| InferenceError::IdentityEncoding)?;
    limits.input_bytes = bytes.len();
    let expected = oracle(&selected, &rules, &seeds, &limits)?;
    if discover_rules(&selected, &rules, &seeds, &limits, 1)? != expected {
        return Err(InferenceError::InvalidInput);
    }
    limits.input_bytes = limits
        .input_bytes
        .checked_sub(1)
        .ok_or(InferenceError::InvalidInput)?;
    if discover_rules(&selected, &rules, &seeds, &limits, 1) != Err(InferenceError::BudgetExhausted)
    {
        return Err(InferenceError::InvalidInput);
    }
    limits.input_bytes = bytes.len();
    for control in 0..4 {
        let mut zero = limits;
        match control {
            0 => zero.facts = 0,
            1 => zero.edge_visits = 0,
            2 => zero.depth = 0,
            _ => zero.claims = 0,
        }
        if discover_rules(&selected, &rules, &[], &zero, 1).is_ok() {
            return Err(InferenceError::InvalidInput);
        }
    }
    Ok(())
}
