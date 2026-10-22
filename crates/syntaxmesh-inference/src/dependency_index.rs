use crate::{Claim, ClaimSelection, InferenceError};
use std::collections::{BTreeMap, BTreeSet};
use syntaxmesh_core::{FactVersionRef, StableId};

/// Derived, selection-scoped dependency lookup; canonical witnesses stay authoritative.
pub struct PremiseDependencyIndex {
    selection: Option<ClaimSelection>,
    postings: BTreeMap<FactVersionRef, BTreeSet<StableId>>,
}

impl PremiseDependencyIndex {
    /// Explain exact changed premise versions for every directly affected claim.
    ///
    /// # Errors
    /// Rejects foreign selection, zero bounds or exhausted work/output/evidence limits.
    pub fn explain_affected_scoped(
        &self,
        selection: ClaimSelection,
        changed: &BTreeSet<FactVersionRef>,
        visit_budget: usize,
        output_budget: usize,
        evidence_budget: usize,
    ) -> Result<BTreeMap<StableId, BTreeSet<FactVersionRef>>, InferenceError> {
        if self.selection != Some(selection)
            || visit_budget == 0
            || output_budget == 0
            || evidence_budget == 0
        {
            return Err(InferenceError::InvalidInput);
        }
        let mut remaining_visits = visit_budget;
        let mut remaining_evidence = evidence_budget;
        let mut explanations = BTreeMap::<StableId, BTreeSet<FactVersionRef>>::new();
        for premise in changed {
            remaining_visits = remaining_visits
                .checked_sub(1)
                .ok_or(InferenceError::BudgetExhausted)?;
            for claim in self.postings.get(premise).into_iter().flatten() {
                remaining_visits = remaining_visits
                    .checked_sub(1)
                    .ok_or(InferenceError::BudgetExhausted)?;
                remaining_evidence = remaining_evidence
                    .checked_sub(1)
                    .ok_or(InferenceError::BudgetExhausted)?;
                explanations.entry(*claim).or_default().insert(*premise);
                if explanations.len() > output_budget {
                    return Err(InferenceError::BudgetExhausted);
                }
            }
        }
        Ok(explanations)
    }
    /// Build a checked index, retaining selection even for an empty claim set.
    ///
    /// # Errors
    /// Rejects foreign claims, malformed witnesses and exhausted build limits.
    pub fn build_scoped(
        selection: ClaimSelection,
        claims: &[Claim],
        claim_budget: usize,
        premise_budget: usize,
    ) -> Result<Self, InferenceError> {
        if claim_budget == 0 || premise_budget == 0 {
            return Err(InferenceError::InvalidInput);
        }
        if claims.len() > claim_budget {
            return Err(InferenceError::BudgetExhausted);
        }
        if claims.iter().any(|claim| {
            claim.repository != selection.repository
                || claim.worktree != selection.worktree
                || claim.generation != selection.generation
        }) {
            return Err(InferenceError::InvalidInput);
        }
        let mut index = Self::build(claims, claim_budget, premise_budget)?;
        index.selection = Some(selection);
        Ok(index)
    }

    /// Match changes only against the explicitly selected claim set.
    ///
    /// # Errors
    /// Rejects foreign/unbound selections and exhausted lookup bounds.
    pub fn affected_scoped(
        &self,
        selection: ClaimSelection,
        changed: &BTreeSet<FactVersionRef>,
        visit_budget: usize,
        output_budget: usize,
    ) -> Result<BTreeSet<StableId>, InferenceError> {
        if self.selection != Some(selection) {
            return Err(InferenceError::InvalidInput);
        }
        self.affected(changed, visit_budget, output_budget)
    }
    /// Index one complete selected claim set with explicit build-work bounds.
    ///
    /// # Errors
    /// Rejects mixed scopes, duplicate identities, empty witnesses and exhaustion.
    pub fn build(
        claims: &[Claim],
        claim_budget: usize,
        premise_budget: usize,
    ) -> Result<Self, InferenceError> {
        if claim_budget == 0 || premise_budget == 0 {
            return Err(InferenceError::InvalidInput);
        }
        if claims.len() > claim_budget {
            return Err(InferenceError::BudgetExhausted);
        }
        let scope = claims
            .first()
            .map(|claim| (claim.repository, claim.worktree, claim.generation));
        let mut identities = BTreeSet::new();
        let mut postings = BTreeMap::<FactVersionRef, BTreeSet<StableId>>::new();
        let mut remaining = premise_budget;
        for claim in claims {
            if scope != Some((claim.repository, claim.worktree, claim.generation))
                || !identities.insert(claim.id)
                || claim.premises.is_empty()
            {
                return Err(InferenceError::InvalidInput);
            }
            for premise in &claim.premises {
                remaining = remaining
                    .checked_sub(1)
                    .ok_or(InferenceError::BudgetExhausted)?;
                postings.entry(*premise).or_default().insert(claim.id);
            }
        }
        Ok(Self {
            selection: scope.map(|(repository, worktree, generation)| ClaimSelection {
                repository,
                worktree,
                generation,
            }),
            postings,
        })
    }

    /// Return the complete affected identity set without replay or rule execution.
    ///
    /// # Errors
    /// Rejects zero bounds or exhausted lookup/posting/output work.
    pub fn affected(
        &self,
        changed: &BTreeSet<FactVersionRef>,
        visit_budget: usize,
        output_budget: usize,
    ) -> Result<BTreeSet<StableId>, InferenceError> {
        if visit_budget == 0 || output_budget == 0 {
            return Err(InferenceError::InvalidInput);
        }
        let mut remaining = visit_budget;
        let mut affected = BTreeSet::new();
        for premise in changed {
            remaining = remaining
                .checked_sub(1)
                .ok_or(InferenceError::BudgetExhausted)?;
            for claim in self.postings.get(premise).into_iter().flatten() {
                remaining = remaining
                    .checked_sub(1)
                    .ok_or(InferenceError::BudgetExhausted)?;
                affected.insert(*claim);
                if affected.len() > output_budget {
                    return Err(InferenceError::BudgetExhausted);
                }
            }
        }
        Ok(affected)
    }
}

#[cfg(test)]
mod tests;
