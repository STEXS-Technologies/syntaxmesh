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
        if self.source_span_augmented {
            return self.source_content_seed_plan(store, generation, request, 32);
        }
        let channels = self.family_channels(store, generation, request, None, 32)?;
        let mut plan = ordering::refill(&channels, 32);
        self.prioritize_primary(&mut plan)?;
        Ok(plan)
    }

    fn source_content_seed_plan<S: GraphStore + ?Sized>(
        &self,
        store: &S,
        generation: GenerationId,
        request: &LexicalPlanRequest<'_>,
        limit: usize,
    ) -> Result<Vec<NodeId>, QueryError> {
        let scores = |channel, posting_budget| LexicalCandidateRequest {
            query: request.query,
            channel,
            family: None,
            eligible: None,
            posting_budget,
            limit,
        };
        let exact = self.lexical_channel_scores(
            store,
            generation,
            &scores(LexicalChannel::Exact, request.posting_budget),
        )?;
        let stem_budget = request
            .posting_budget
            .checked_sub(exact.1)
            .ok_or(QueryError::InvalidLimit)?;
        let stemmed = self.lexical_channel_scores(
            store,
            generation,
            &scores(LexicalChannel::Stemmed, stem_budget),
        )?;
        let data = self.planner.as_ref().ok_or(QueryError::InvalidLimit)?;
        Ok(source_content_seed_selection(
            &exact.0, &stemmed.0, data, limit,
        ))
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
                let in_family =
                    |entry: &&(NodeId, u64)| data.families.get(&entry.0) == Some(&family);
                let mut channel = Vec::new();
                if self.source_span_augmented {
                    let mut seen_files = BTreeSet::new();
                    let mut selected_nodes = BTreeSet::new();
                    for (id, _) in ranked.iter().filter(in_family) {
                        if let Some(file_id) = data.source_files.get(id)
                            && !seen_files.insert(*file_id)
                        {
                            continue;
                        }
                        if selected_nodes.insert(*id) {
                            channel.push(*id);
                        }
                        if channel.len() >= limit {
                            break;
                        }
                    }
                    if channel.len() < limit {
                        for (id, _) in ranked.iter().filter(in_family) {
                            if selected_nodes.insert(*id) {
                                channel.push(*id);
                            }
                            if channel.len() >= limit {
                                break;
                            }
                        }
                    }
                } else {
                    channel.extend(
                        ranked
                            .iter()
                            .filter(in_family)
                            .take(limit)
                            .map(|entry| entry.0),
                    );
                }
                channels.push(channel);
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

fn source_content_seed_selection(
    exact: &[(NodeId, u64)],
    stemmed: &[(NodeId, u64)],
    data: &super::lexical::PlannerData,
    limit: usize,
) -> Vec<NodeId> {
    let mut scores = std::collections::BTreeMap::<NodeId, u64>::new();
    for (id, score) in exact.iter().chain(stemmed) {
        scores
            .entry(*id)
            .and_modify(|current| *current = (*current).max(*score))
            .or_insert(*score);
    }
    let mut primary = Vec::new();
    let mut secondary = Vec::new();
    for (id, score) in scores {
        match data.families.get(&id) {
            Some(LexicalFamily::Code | LexicalFamily::Documentation) => {
                primary.push((id, score));
            }
            Some(LexicalFamily::Reference | LexicalFamily::Other) => {
                secondary.push((id, score));
            }
            None => {}
        }
    }
    let rank = |left: &(NodeId, u64), right: &(NodeId, u64)| {
        right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0))
    };
    primary.sort_by(rank);
    secondary.sort_by(rank);

    let diversity_limit = limit.div_ceil(2).min(limit);
    let mut selected = Vec::with_capacity(limit.min(256));
    let mut selected_ids = std::collections::BTreeSet::new();
    let mut selected_files = std::collections::BTreeSet::new();
    for (id, _) in &primary {
        let new_file = data
            .source_files
            .get(id)
            .is_none_or(|file_id| selected_files.insert(*file_id));
        if new_file && selected_ids.insert(*id) {
            selected.push(*id);
        }
        if selected.len() >= diversity_limit {
            break;
        }
    }
    for (id, _) in primary.iter().chain(&secondary) {
        if selected.len() >= limit.min(256) {
            break;
        }
        if selected_ids.insert(*id) {
            selected.push(*id);
        }
    }
    selected
}

#[cfg(test)]
mod tests;
