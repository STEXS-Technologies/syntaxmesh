use crate::{
    Claim, ClaimSelection, DerivationClass, InferenceError, Limits, Premise, Rule, Selection,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use syntaxmesh_core::{FactRef, FactVersionRef, NodeId, RelationKind};

fn visit(remaining: &mut usize) -> Result<(), InferenceError> {
    *remaining = remaining
        .checked_sub(1)
        .ok_or(InferenceError::BudgetExhausted)?;
    Ok(())
}

const fn reference(premise: &Premise) -> FactVersionRef {
    FactVersionRef {
        fact: FactRef::Edge(premise.edge.id),
        valid_from: premise.valid_from,
    }
}

#[cfg(test)]
pub(crate) mod tests;

/// Evaluate complete bounded witnesses; any exhausted bound rejects the result.
///
/// # Errors
/// Rejects malformed/mixed-generation premises, invalid rules and exhausted work.
pub fn evaluate(
    selection: &Selection<'_>,
    rule: &Rule,
    limits: &Limits,
) -> Result<Vec<Claim>, InferenceError> {
    crate::evaluate_rules(selection, std::slice::from_ref(rule), limits, 1)
}

pub(crate) fn evaluate_into(
    selection: &Selection<'_>,
    rule: &Rule,
    limits: &Limits,
    remaining: &mut usize,
    claims: &mut Vec<Claim>,
) -> Result<(), InferenceError> {
    evaluate_scoped(
        selection,
        rule,
        limits,
        remaining,
        claims,
        EvaluationScope {
            starts: None,
            seeds: None,
        },
    )
}

#[derive(Clone, Copy)]
pub(crate) struct EvaluationScope<'filter> {
    pub starts: Option<&'filter BTreeSet<NodeId>>,
    pub seeds: Option<&'filter BTreeSet<FactVersionRef>>,
}

pub(crate) fn evaluate_scoped(
    selection: &Selection<'_>,
    rule: &Rule,
    limits: &Limits,
    remaining: &mut usize,
    claims: &mut Vec<Claim>,
    scope: EvaluationScope<'_>,
) -> Result<(), InferenceError> {
    if limits.facts == 0
        || limits.edge_visits == 0
        || limits.depth == 0
        || limits.claims == 0
        || rule.id.is_empty()
        || rule.version.is_empty()
        || !selection.nodes.contains(&rule.concept)
        || !matches!(&rule.output_relation, RelationKind::External { namespace, relation } if !namespace.is_empty() && namespace != "core" && !relation.is_empty())
    {
        return Err(InferenceError::InvalidInput);
    }
    if selection
        .nodes
        .len()
        .checked_add(selection.premises.len())
        .is_none_or(|count| count > limits.facts)
    {
        return Err(InferenceError::BudgetExhausted);
    }
    let mut adjacency = BTreeMap::<NodeId, Vec<&Premise>>::new();
    let mut identities = BTreeSet::new();
    for premise in selection.premises {
        if premise.generation != selection.generation
            || !identities.insert(premise.edge.id)
            || !selection.nodes.contains(&premise.edge.source)
            || !selection.nodes.contains(&premise.edge.target)
        {
            return Err(InferenceError::InvalidInput);
        }
        adjacency
            .entry(premise.edge.source)
            .or_default()
            .push(premise);
    }
    for edges in adjacency.values_mut() {
        edges.sort_by_key(|premise| premise.edge.id);
    }
    let starts = scope.starts.unwrap_or(selection.nodes);
    if !starts.is_subset(selection.nodes) {
        return Err(InferenceError::InvalidInput);
    }
    for start in starts {
        let mut queue = VecDeque::from([(*start, vec![*start], Vec::<FactVersionRef>::new())]);
        while let Some((current, visited, witness)) = queue.pop_front() {
            for next in adjacency.get(&current).into_iter().flatten() {
                visit(remaining)?;
                if next.edge.relation != rule.path_relation || visited.contains(&next.edge.target) {
                    continue;
                }
                if witness.len() >= limits.depth {
                    return Err(InferenceError::DepthExhausted);
                }
                let mut path = witness.clone();
                path.push(reference(next));
                for join in adjacency.get(&next.edge.target).into_iter().flatten() {
                    visit(remaining)?;
                    if join.edge.relation != rule.join_relation {
                        continue;
                    }
                    for declaration in adjacency.get(&join.edge.target).into_iter().flatten() {
                        visit(remaining)?;
                        if declaration.edge.relation != rule.declaration_relation
                            || declaration.edge.target != rule.concept
                        {
                            continue;
                        }
                        let mut premises = path.clone();
                        premises.extend([reference(join), reference(declaration)]);
                        if scope.seeds.is_some_and(|seeds| {
                            !premises.iter().any(|premise| seeds.contains(premise))
                        }) {
                            continue;
                        }
                        if claims.len() >= limits.claims {
                            return Err(InferenceError::BudgetExhausted);
                        }
                        let id = crate::witness::claim_identity(
                            ClaimSelection {
                                repository: selection.repository,
                                worktree: selection.worktree,
                                generation: selection.generation,
                            },
                            rule,
                            *start,
                            join.edge.target,
                            &premises,
                        )?;
                        claims.push(Claim {
                            id,
                            derivation_class: DerivationClass::DeterministicRule,
                            repository: selection.repository,
                            worktree: selection.worktree,
                            generation: selection.generation,
                            source: *start,
                            target: join.edge.target,
                            relation: rule.output_relation.clone(),
                            rule_id: rule.id.clone(),
                            rule_version: rule.version.clone(),
                            ontology_versions: rule.ontology_versions.clone(),
                            configuration_fingerprint: rule.configuration_fingerprint,
                            premises,
                        });
                    }
                }
                let mut nodes = visited.clone();
                nodes.push(next.edge.target);
                queue.push_back((next.edge.target, nodes, path));
            }
        }
    }
    Ok(())
}
