use crate::evaluator::{EvaluationScope, evaluate_scoped};
use crate::{Claim, InferenceError, Limits, Premise, Rule, Selection};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use syntaxmesh_core::{FactRef, FactVersionRef, NodeId};

/// Discover bounded witnesses containing an exact supplied active edge version.
/// This is a witness delta, never a complete selection or a publication receipt.
///
/// # Errors
/// Rejects malformed seeds/inputs and exhausted admission, traversal or output bounds.
pub fn discover_rules(
    selection: &Selection<'_>,
    rules: &[Rule],
    seeds: &[FactVersionRef],
    limits: &Limits,
    rule_budget: usize,
) -> Result<Vec<Claim>, InferenceError> {
    discover_rules_with_budget(
        selection,
        rules,
        seeds,
        limits,
        rule_budget,
        limits.edge_visits,
    )
    .map(|(claims, _remaining)| claims)
}

pub(crate) fn discover_rules_with_budget(
    selection: &Selection<'_>,
    rules: &[Rule],
    seeds: &[FactVersionRef],
    limits: &Limits,
    rule_budget: usize,
    edge_visit_budget: usize,
) -> Result<(Vec<Claim>, usize), InferenceError> {
    let mut remaining = edge_visit_budget;
    let ordered =
        crate::rule_set::admit_rules(selection, rules, limits, rule_budget, &mut remaining)?;
    if seeds.len() > selection.premises.len() {
        return Err(InferenceError::InvalidInput);
    }
    crate::input_budget::validate_encoded(
        &(
            selection.repository,
            selection.worktree,
            selection.generation,
            selection.nodes,
            selection.premises,
            rules,
            seeds,
        ),
        limits.input_bytes,
    )?;
    // Ordinary validation must still run for empty seeds and irrelevant rules.
    let empty = BTreeSet::new();
    let mut claims = Vec::new();
    for rule in &ordered {
        evaluate_scoped(
            selection,
            rule,
            limits,
            &mut remaining,
            &mut claims,
            EvaluationScope {
                starts: Some(&empty),
                seeds: None,
            },
        )?;
    }
    let active = selection
        .premises
        .iter()
        .map(|premise| {
            (
                FactVersionRef {
                    fact: FactRef::Edge(premise.edge.id),
                    valid_from: premise.valid_from,
                },
                premise,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut exact = BTreeSet::new();
    for seed in seeds {
        charge(&mut remaining)?;
        if !active.contains_key(seed) || !exact.insert(*seed) {
            return Err(InferenceError::InvalidInput);
        }
    }
    if exact.is_empty() {
        return Ok((claims, remaining));
    }
    let mut reverse = BTreeMap::<NodeId, Vec<&Premise>>::new();
    for premise in selection.premises {
        charge(&mut remaining)?;
        reverse
            .entry(premise.edge.target)
            .or_default()
            .push(premise);
    }
    for incoming in reverse.values_mut() {
        incoming.sort_by_key(|premise| premise.edge.id);
    }
    for rule in ordered {
        let mut starts = BTreeSet::new();
        let mut walker = Walker {
            reverse: &reverse,
            rule,
            remaining: &mut remaining,
        };
        for seed in &exact {
            charge(walker.remaining)?;
            let premise = active.get(seed).ok_or(InferenceError::InvalidInput)?;
            let edge = &premise.edge;
            if edge.relation == rule.path_relation && edge.source != edge.target {
                starts.extend(
                    walker.ancestors(
                        edge.source,
                        limits
                            .depth
                            .checked_sub(1)
                            .ok_or(InferenceError::InvalidInput)?,
                        true,
                        Some(edge.target),
                    )?,
                );
            }
            if edge.relation == rule.join_relation {
                starts.extend(walker.ancestors(edge.source, limits.depth, false, None)?);
            }
            if edge.relation == rule.declaration_relation && edge.target == rule.concept {
                for join in reverse.get(&edge.source).into_iter().flatten() {
                    charge(walker.remaining)?;
                    if join.edge.relation == rule.join_relation {
                        starts.extend(walker.ancestors(
                            join.edge.source,
                            limits.depth,
                            false,
                            None,
                        )?);
                    }
                }
            }
        }
        evaluate_scoped(
            selection,
            rule,
            limits,
            &mut remaining,
            &mut claims,
            EvaluationScope {
                starts: Some(&starts),
                seeds: Some(&exact),
            },
        )?;
    }
    claims.sort_by_key(|claim| claim.id);
    Ok((claims, remaining))
}

fn charge(remaining: &mut usize) -> Result<(), InferenceError> {
    *remaining = remaining
        .checked_sub(1)
        .ok_or(InferenceError::BudgetExhausted)?;
    Ok(())
}

struct Walker<'facts, 'work> {
    reverse: &'facts BTreeMap<NodeId, Vec<&'facts Premise>>,
    rule: &'facts Rule,
    remaining: &'work mut usize,
}

impl Walker<'_, '_> {
    fn ancestors(
        &mut self,
        root: NodeId,
        maximum: usize,
        include_root: bool,
        excluded: Option<NodeId>,
    ) -> Result<BTreeSet<NodeId>, InferenceError> {
        let mut seen = BTreeSet::from([root]);
        let mut result = BTreeSet::new();
        if include_root {
            result.insert(root);
        }
        let mut queue = VecDeque::from([(root, 0_usize)]);
        while let Some((current, depth)) = queue.pop_front() {
            for incoming in self.reverse.get(&current).into_iter().flatten() {
                charge(self.remaining)?;
                let source = incoming.edge.source;
                if incoming.edge.relation != self.rule.path_relation
                    || excluded == Some(source)
                    || seen.contains(&source)
                {
                    continue;
                }
                if depth >= maximum {
                    return Err(InferenceError::DepthExhausted);
                }
                seen.insert(source);
                result.insert(source);
                queue.push_back((
                    source,
                    depth.checked_add(1).ok_or(InferenceError::DepthExhausted)?,
                ));
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
