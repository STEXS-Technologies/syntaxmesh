use crate::{Query, QueryError};
use std::collections::{BTreeMap, BTreeSet};
use syntaxmesh_core::{FactPayload, FactRef, FactVersionRef, GenerationId, StableId};
use syntaxmesh_inference::{
    ClaimSelection, InvalidationLimits, PremiseClosure, PremiseDependencyIndex,
};
use syntaxmesh_store::GraphStore;

pub(super) struct PremiseTransitions {
    pub closed: BTreeMap<FactVersionRef, PremiseClosure>,
    pub opened: BTreeSet<FactVersionRef>,
}

impl<S: GraphStore + ?Sized> Query<'_, S> {
    /// Identify directly affected retained claims over a bounded accepted range.
    ///
    /// # Errors
    /// Rejects foreign indices, reversed/malformed feeds and exhausted limits.
    pub fn inference_invalidations(
        &self,
        index: &PremiseDependencyIndex,
        from: GenerationId,
        limits: &InvalidationLimits,
    ) -> Result<BTreeSet<StableId>, QueryError> {
        let (scope, closed) = self.inference_closed_versions(index, from, limits)?;
        index
            .affected_scoped(scope, &closed, limits.postings, limits.claims)
            .map_err(|error| QueryError::Context(error.to_string()))
    }

    /// Explain exact closed premise versions behind direct range invalidations.
    ///
    /// # Errors
    /// Rejects malformed feeds, foreign scope or exhausted work/output/evidence bounds.
    pub fn inference_invalidation_explanations(
        &self,
        index: &PremiseDependencyIndex,
        from: GenerationId,
        limits: &InvalidationLimits,
        evidence_budget: usize,
    ) -> Result<BTreeMap<StableId, BTreeSet<FactVersionRef>>, QueryError> {
        if evidence_budget == 0 {
            return Err(QueryError::InvalidLimit);
        }
        let (scope, closed) = self.inference_closed_versions(index, from, limits)?;
        index
            .explain_affected_scoped(
                scope,
                &closed,
                limits.postings,
                limits.claims,
                evidence_budget,
            )
            .map_err(|error| QueryError::Context(error.to_string()))
    }

    fn inference_closed_versions(
        &self,
        index: &PremiseDependencyIndex,
        from: GenerationId,
        limits: &InvalidationLimits,
    ) -> Result<(ClaimSelection, BTreeSet<FactVersionRef>), QueryError> {
        if limits.generations == 0
            || limits.versions == 0
            || limits.postings == 0
            || limits.claims == 0
        {
            return Err(QueryError::InvalidLimit);
        }
        let retained = self.store.manifest(from)?;
        let scope = ClaimSelection {
            repository: retained.repository,
            worktree: retained.worktree,
            generation: from,
        };
        index
            .affected_scoped(scope, &BTreeSet::new(), limits.postings, limits.claims)
            .map_err(|error| QueryError::Context(error.to_string()))?;
        let closed = self
            .inference_premise_closures(from, limits)?
            .into_keys()
            .collect();
        Ok((scope, closed))
    }

    /// Retain the exact accepted closure time for canonical premise versions.
    ///
    /// # Errors
    /// Rejects foreign/reversed ranges, malformed feeds and exhausted work bounds.
    pub fn inference_premise_closures(
        &self,
        from: GenerationId,
        limits: &InvalidationLimits,
    ) -> Result<BTreeMap<FactVersionRef, PremiseClosure>, QueryError> {
        if limits.generations == 0
            || limits.versions == 0
            || limits.postings == 0
            || limits.claims == 0
        {
            return Err(QueryError::InvalidLimit);
        }
        Ok(self
            .inference_premise_transitions(from, limits.generations, limits.versions)?
            .closed)
    }

    pub(super) fn inference_premise_transitions(
        &self,
        from: GenerationId,
        generation_budget: usize,
        version_budget: usize,
    ) -> Result<PremiseTransitions, QueryError> {
        if generation_budget == 0 || version_budget == 0 {
            return Err(QueryError::InvalidLimit);
        }
        let selected = self.manifest()?;
        let retained = self.store.manifest(from)?;
        if retained.repository != selected.repository || retained.worktree != selected.worktree {
            return Err(QueryError::InvalidLimit);
        }
        let scope = ClaimSelection {
            repository: retained.repository,
            worktree: retained.worktree,
            generation: from,
        };
        let mut previous_sequence = self.store.generation_sequence(from)?;
        let end_sequence = self.store.generation_sequence(self.generation)?;
        if previous_sequence > end_sequence {
            return Err(QueryError::InvalidLimit);
        }
        if from == self.generation {
            return Ok(PremiseTransitions {
                closed: BTreeMap::new(),
                opened: BTreeSet::new(),
            });
        }
        let mut previous_generation = from;
        let mut cursor = None;
        let mut remaining_generations = generation_budget;
        let mut remaining_versions = version_budget;
        let mut closed = BTreeMap::new();
        let mut opened = BTreeSet::new();
        loop {
            remaining_generations = remaining_generations
                .checked_sub(1)
                .ok_or(QueryError::InvalidLimit)?;
            let page = self
                .store
                .changes_between_page(from, self.generation, cursor, 1)?;
            let change = page.items.first().ok_or(QueryError::InvalidLimit)?;
            let mut retained_manifest = self.store.manifest(change.manifest.generation)?;
            // Verification status is operational, not immutable graph identity.
            retained_manifest.status = change.manifest.status;
            if page.items.len() != 1
                || change.sequence
                    != previous_sequence
                        .checked_add(1)
                        .ok_or(QueryError::InvalidLimit)?
                || change.sequence > end_sequence
                || change.delta.expected_base != Some(previous_generation)
                || change.manifest.parent != Some(previous_generation)
                || change.sequence != self.store.generation_sequence(change.manifest.generation)?
                || change.manifest != retained_manifest
                || change.manifest.repository != scope.repository
                || change.manifest.worktree != scope.worktree
                || change.delta.repository != scope.repository
                || change.delta.worktree != scope.worktree
                || change.delta.next_generation != change.manifest.generation
            {
                return Err(QueryError::InvalidLimit);
            }
            let generation = change.manifest.generation;
            let mut version_cursor = None;
            let mut previous_version = None;
            loop {
                // One bounded probe distinguishes an empty transition from an
                // additional version after the exact entry budget is consumed.
                let limit = remaining_versions.clamp(1, 100);
                let versions =
                    self.store
                        .fact_version_changes_at_page(generation, version_cursor, limit)?;
                if versions.items.len() > limit {
                    return Err(QueryError::InvalidLimit);
                }
                for entry in &versions.items {
                    remaining_versions = remaining_versions
                        .checked_sub(1)
                        .ok_or(QueryError::InvalidLimit)?;
                    let payload_fact = match &entry.version.payload {
                        FactPayload::File(file) => FactRef::File(file.file_id),
                        FactPayload::Provenance(provenance) => FactRef::Provenance(provenance.id),
                        FactPayload::Node(node) => FactRef::Node(node.id),
                        FactPayload::Edge(edge) => FactRef::Edge(edge.id),
                    };
                    let key = (entry.fact, entry.valid_from_sequence);
                    if entry.fact != payload_fact
                        || entry.valid_from_sequence
                            != self.store.generation_sequence(entry.version.valid_from)?
                        || previous_version.is_some_and(|previous| previous >= key)
                        || entry.valid_from_sequence == 0
                        || entry.valid_from_sequence > change.sequence
                        || entry.valid_until_sequence.is_some()
                            != entry.version.valid_until.is_some()
                        || entry
                            .valid_until_sequence
                            .is_some_and(|until| until <= entry.valid_from_sequence)
                        || (entry.version.valid_from == generation
                            && entry.valid_from_sequence != change.sequence)
                        || (entry.version.valid_until == Some(generation)
                            && entry.valid_until_sequence != Some(change.sequence))
                        || (entry.version.valid_from != generation
                            && entry.version.valid_until != Some(generation))
                    {
                        return Err(QueryError::InvalidLimit);
                    }
                    if let Some(until) = entry.version.valid_until
                        && entry.valid_until_sequence
                            != Some(self.store.generation_sequence(until)?)
                    {
                        return Err(QueryError::InvalidLimit);
                    }
                    previous_version = Some(key);
                    if entry.version.valid_from == generation
                        && !opened.insert(FactVersionRef {
                            fact: entry.fact,
                            valid_from: generation,
                        })
                    {
                        return Err(QueryError::InvalidLimit);
                    }
                    if entry.version.valid_until == Some(generation) {
                        let reference = FactVersionRef {
                            fact: entry.fact,
                            valid_from: entry.version.valid_from,
                        };
                        if closed
                            .insert(
                                reference,
                                PremiseClosure {
                                    generation,
                                    generation_sequence: change.sequence,
                                },
                            )
                            .is_some()
                        {
                            return Err(QueryError::InvalidLimit);
                        }
                    }
                }
                let Some(next) = versions.next_cursor else {
                    break;
                };
                let last = versions.items.last().ok_or(QueryError::InvalidLimit)?;
                if next.generation != generation
                    || Some(next) == version_cursor
                    || next.after_fact != last.fact
                    || next.after_valid_from_sequence != last.valid_from_sequence
                {
                    return Err(QueryError::InvalidLimit);
                }
                version_cursor = Some(next);
            }
            previous_sequence = change.sequence;
            previous_generation = generation;
            let Some(next) = page.next_cursor else {
                break;
            };
            if next.from_generation != from
                || next.to_generation != self.generation
                || Some(next) == cursor
                || next.after_sequence != previous_sequence
                || next.after_generation != previous_generation
            {
                return Err(QueryError::InvalidLimit);
            }
            cursor = Some(next);
        }
        if previous_generation != self.generation || previous_sequence != end_sequence {
            return Err(QueryError::InvalidLimit);
        }
        Ok(PremiseTransitions { closed, opened })
    }
}
