use super::*;
use crate::evaluator::tests::{fixture, selection};
use syntaxmesh_core::GenerationId;

#[test]
fn explanations_match_witness_intersections_with_independent_evidence_bounds()
-> Result<(), InferenceError> {
    let (nodes, premises, rule, limits) = fixture();
    let selected = selection(&nodes, &premises);
    let scope = ClaimSelection {
        repository: selected.repository,
        worktree: selected.worktree,
        generation: selected.generation,
    };
    let claims = crate::evaluate(&selected, &rule, &limits)?;
    let index = PremiseDependencyIndex::build_scoped(scope, &claims, 2, 10)?;
    let changed = claims
        .iter()
        .flat_map(|claim| claim.premises.iter().copied())
        .collect::<BTreeSet<_>>();
    let expected = claims
        .iter()
        .map(|claim| {
            (
                claim.id,
                claim
                    .premises
                    .iter()
                    .copied()
                    .filter(|premise| changed.contains(premise))
                    .collect(),
            )
        })
        .collect();
    let explained = index.explain_affected_scoped(scope, &changed, 100, 2, 10)?;
    let evidence_count = explained.values().try_fold(0_usize, |total, reasons| {
        total
            .checked_add(reasons.len())
            .ok_or(InferenceError::InvalidInput)
    })?;
    if explained.values().any(|reasons| reasons.len() < 2)
        || index.explain_affected_scoped(scope, &changed, 100, 2, evidence_count)? != explained
        || index
            .explain_affected_scoped(
                scope,
                &changed,
                100,
                2,
                evidence_count
                    .checked_sub(1)
                    .ok_or(InferenceError::InvalidInput)?,
            )
            .is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    let foreign_scope = ClaimSelection {
        generation: GenerationId::derive(&[b"foreign-explanation"]),
        ..scope
    };
    let empty_index = PremiseDependencyIndex::build_scoped(scope, &[], 1, 1)?;
    if !empty_index
        .explain_affected_scoped(scope, &changed, 100, 1, 1)?
        .is_empty()
        || empty_index
            .explain_affected_scoped(foreign_scope, &changed, 100, 1, 1)
            .is_ok()
        || index
            .explain_affected_scoped(scope, &changed, 100, 2, 0)
            .is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    if explained != expected
        || explained.keys().copied().collect::<BTreeSet<_>>()
            != index.affected_scoped(scope, &changed, 100, 2)?
        || index
            .explain_affected_scoped(scope, &changed, 100, 2, 1)
            .is_ok()
        || index
            .explain_affected_scoped(scope, &changed, 1, 2, 10)
            .is_ok()
        || index
            .explain_affected_scoped(scope, &changed, 100, 1, 10)
            .is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn scoped_lookup_rejects_foreign_selections_and_preserves_empty_scope() -> Result<(), InferenceError>
{
    let (nodes, premises, rule, limits) = fixture();
    let selected = selection(&nodes, &premises);
    let scope = ClaimSelection {
        repository: selected.repository,
        worktree: selected.worktree,
        generation: selected.generation,
    };
    let claims = crate::evaluate(&selected, &rule, &limits)?;
    let changed = claims
        .iter()
        .flat_map(|claim| claim.premises.iter().copied())
        .collect();
    let index = PremiseDependencyIndex::build_scoped(scope, &claims, 2, 10)?;
    if index.affected_scoped(scope, &changed, 100, 2)? != index.affected(&changed, 100, 2)? {
        return Err(InferenceError::InvalidInput);
    }
    let empty = PremiseDependencyIndex::build_scoped(scope, &[], 2, 10)?;
    if !empty.affected_scoped(scope, &changed, 100, 2)?.is_empty() {
        return Err(InferenceError::InvalidInput);
    }
    let foreign = [
        ClaimSelection {
            repository: syntaxmesh_core::RepositoryId::derive(&[b"foreign"]),
            ..scope
        },
        ClaimSelection {
            worktree: syntaxmesh_core::WorktreeId::derive(&[b"foreign"]),
            ..scope
        },
        ClaimSelection {
            generation: GenerationId::derive(&[b"foreign"]),
            ..scope
        },
    ];
    for other in foreign {
        if index.affected_scoped(other, &changed, 100, 2).is_ok()
            || empty.affected_scoped(other, &changed, 100, 2).is_ok()
            || PremiseDependencyIndex::build_scoped(other, &claims, 2, 10).is_ok()
        {
            return Err(InferenceError::InvalidInput);
        }
    }
    Ok(())
}

#[test]
fn dependency_lookup_matches_exact_witness_scan_and_deduplicates_hits() -> Result<(), InferenceError>
{
    let (nodes, premises, rule, limits) = fixture();
    let claims = crate::evaluate(&selection(&nodes, &premises), &rule, &limits)?;
    let index = PremiseDependencyIndex::build(&claims, 2, 10)?;
    let all = claims
        .iter()
        .flat_map(|claim| claim.premises.iter().copied())
        .collect::<BTreeSet<_>>();
    for reference in &all {
        let expected = claims
            .iter()
            .filter(|claim| claim.premises.contains(reference))
            .map(|claim| claim.id)
            .collect();
        if index.affected(&BTreeSet::from([*reference]), 100, 2)? != expected {
            return Err(InferenceError::InvalidInput);
        }
        let foreign = FactVersionRef {
            valid_from: GenerationId::derive(&[b"other-version"]),
            ..*reference
        };
        if !index
            .affected(&BTreeSet::from([foreign]), 100, 2)?
            .is_empty()
        {
            return Err(InferenceError::InvalidInput);
        }
    }
    if index.affected(&all, 100, 2)? != claims.iter().map(|claim| claim.id).collect()
        || index.affected(&all, 1, 2).is_ok()
        || index.affected(&all, 100, 1).is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}

#[test]
fn dependency_build_rejects_mixed_duplicate_and_unbounded_inputs() -> Result<(), InferenceError> {
    let (nodes, premises, rule, limits) = fixture();
    let claims = crate::evaluate(&selection(&nodes, &premises), &rule, &limits)?;
    if PremiseDependencyIndex::build(&claims, 1, 10).is_ok()
        || PremiseDependencyIndex::build(&claims, 2, 1).is_ok()
    {
        return Err(InferenceError::InvalidInput);
    }
    let mut invalid = claims.clone();
    let first = invalid
        .first()
        .cloned()
        .ok_or(InferenceError::InvalidInput)?;
    invalid.push(first);
    if PremiseDependencyIndex::build(&invalid, 3, 20).is_ok() {
        return Err(InferenceError::InvalidInput);
    }
    let mut mixed = claims;
    mixed
        .first_mut()
        .ok_or(InferenceError::InvalidInput)?
        .generation = GenerationId::derive(&[b"mixed"]);
    if PremiseDependencyIndex::build(&mixed, 2, 10).is_ok() {
        return Err(InferenceError::InvalidInput);
    }
    Ok(())
}
