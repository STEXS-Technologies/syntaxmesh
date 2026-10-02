use super::GenerationIdentifierIndex;
use super::lexical::{LexicalCandidateRequest, LexicalChannel, LexicalFamily};
use crate::QueryError;
use std::collections::BTreeSet;
use syntaxmesh_core::{GenerationId, NodeId};
use syntaxmesh_store::GraphStore;

mod ordering;

/// Explicit discovery ordering; lexical seeds are not a confidence guarantee.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiscoveryPackingPreference {
    #[default]
    Balanced,
    LexicalFirst,
}

/// Explicit ordering preference; absence of test evidence is not production proof.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SourceRolePreference {
    #[default]
    Neutral,
    TestIntentFirst,
    TestIntentLast,
}

/// Complete membership and explicit role ordering for a packing plan.
pub struct SourceRolePackingRequest<'ids> {
    pub selected: &'ids BTreeSet<NodeId>,
    pub original: &'ids BTreeSet<NodeId>,
    pub preference: SourceRolePreference,
}

/// One aggregate visit budget across the eight family/channel queries.
pub struct LexicalPlanRequest<'query> {
    pub query: &'query str,
    pub posting_budget: usize,
}

impl GenerationIdentifierIndex {
    /// Compose discovery preference outside the existing stable role ordering.
    ///
    /// # Errors
    /// Preserves complete-plan membership, generation and posting-budget checks.
    pub fn lexical_packing_plan_with_discovery<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        request: &LexicalPlanRequest<'_>,
        roles: &SourceRolePackingRequest<'_>,
        preference: DiscoveryPackingPreference,
    ) -> Result<Vec<NodeId>, QueryError> {
        let mut plan = self.lexical_packing_plan_with_roles(store, generation, request, roles)?;
        if preference == DiscoveryPackingPreference::LexicalFirst {
            plan.sort_by_key(|id| !roles.original.contains(id));
        }
        Ok(plan)
    }
    /// Stable role partition of the complete existing packing plan.
    ///
    /// # Errors
    /// Preserves ordinary plan validation, generation and posting-budget errors.
    pub fn lexical_packing_plan_with_roles<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        request: &LexicalPlanRequest<'_>,
        roles: &SourceRolePackingRequest<'_>,
    ) -> Result<Vec<NodeId>, QueryError> {
        let mut plan =
            self.lexical_packing_plan(store, generation, request, roles.selected, roles.original)?;
        let data = self.planner.as_ref().ok_or(QueryError::InvalidLimit)?;
        match roles.preference {
            SourceRolePreference::Neutral => {}
            SourceRolePreference::TestIntentFirst => {
                plan.sort_by_key(|id| !data.test_intent.contains(id))
            }
            SourceRolePreference::TestIntentLast => {
                plan.sort_by_key(|id| data.test_intent.contains(id))
            }
        }
        Ok(plan)
    }
    /// Compose at most 32 ranked lexical seed IDs without canonical hydration.
    ///
    /// # Errors
    /// Rejects missing index capability, stale identity and exhausted work.
    pub fn lexical_seed_plan<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        request: &LexicalPlanRequest<'_>,
    ) -> Result<Vec<NodeId>, QueryError> {
        let channels = self.family_channels(store, generation, request, None, 32)?;
        let mut plan = ordering::refill(&channels, 32);
        self.prioritize_primary(&mut plan)?;
        Ok(plan)
    }

    /// Order every selected ID, retaining zero-score discoveries and membership.
    /// No graph traversal or canonical hydration occurs during composition.
    ///
    /// # Errors
    /// Rejects foreign/oversized sets, invalid original seeds, stale identity,
    /// missing index capability and exhausted aggregate posting work.
    pub fn lexical_packing_plan<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        request: &LexicalPlanRequest<'_>,
        selected: &BTreeSet<NodeId>,
        original: &BTreeSet<NodeId>,
    ) -> Result<Vec<NodeId>, QueryError> {
        if selected.len() > 256 || original.len() > 32 || !original.is_subset(selected) {
            return Err(QueryError::InvalidLimit);
        }
        let channels = self.family_channels(
            store,
            generation,
            request,
            Some(selected),
            selected.len().max(1),
        )?;
        let mut complete = ordering::refill(&channels, selected.len());
        let ranked = complete.iter().copied().collect::<BTreeSet<_>>();
        complete.extend(selected.difference(&ranked).copied());
        self.prioritize_primary(&mut complete)?;
        let lexical = complete
            .iter()
            .filter(|id| original.contains(id))
            .copied()
            .collect();
        let discovered = complete
            .iter()
            .filter(|id| !original.contains(id))
            .copied()
            .collect();
        let mut plan = ordering::refill(&[lexical, discovered], selected.len());
        self.prioritize_primary(&mut plan)?;
        Ok(plan)
    }

    fn family_channels<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        request: &LexicalPlanRequest<'_>,
        selected: Option<&BTreeSet<NodeId>>,
        limit: usize,
    ) -> Result<Vec<Vec<NodeId>>, QueryError> {
        let score_request = |channel, posting_budget| LexicalCandidateRequest {
            query: request.query,
            channel,
            family: None,
            eligible: selected,
            posting_budget,
            limit,
        };
        let exact = self.lexical_channel_scores(
            store,
            generation,
            &score_request(LexicalChannel::Exact, request.posting_budget),
        )?;
        let stem_budget = request
            .posting_budget
            .checked_sub(exact.1)
            .ok_or(QueryError::InvalidLimit)?;
        let stemmed = self.lexical_channel_scores(
            store,
            generation,
            &score_request(LexicalChannel::Stemmed, stem_budget),
        )?;
        let data = self.planner.as_ref().ok_or(QueryError::InvalidLimit)?;
        let mut remaining = request.posting_budget;
        let mut channels = Vec::new();
        for family in [
            LexicalFamily::Code,
            LexicalFamily::Documentation,
            LexicalFamily::Reference,
            LexicalFamily::Other,
        ] {
            for (ranked, visits) in [&exact, &stemmed] {
                if remaining == 0 {
                    return Err(QueryError::InvalidLimit);
                }
                remaining = remaining.checked_sub(*visits).ok_or_else(|| {
                    QueryError::Context("lexical candidate posting budget exhausted".to_owned())
                })?;
                channels.push(
                    ranked
                        .iter()
                        .filter(|entry| data.families.get(&entry.0) == Some(&family))
                        .take(limit)
                        .map(|entry| entry.0)
                        .collect(),
                );
            }
        }
        Ok(channels)
    }

    fn prioritize_primary(&self, ids: &mut [NodeId]) -> Result<(), QueryError> {
        let data = self.planner.as_ref().ok_or(QueryError::InvalidLimit)?;
        ids.sort_by_key(|id| {
            !matches!(
                data.families.get(id),
                Some(LexicalFamily::Code | LexicalFamily::Documentation)
            )
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
