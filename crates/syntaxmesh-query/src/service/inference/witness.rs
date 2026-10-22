use crate::{Query, QueryError};
use syntaxmesh_core::{FactPayload, FactRef};
use syntaxmesh_inference::{Claim, Premise, Rule};
use syntaxmesh_store::{FactHistoryEntry, FactHistoryPage, GraphStore};

impl<S: GraphStore + ?Sized> Query<'_, S> {
    /// Verify a supported rule witness using authoritative active premise versions.
    ///
    /// # Errors
    /// Rejects foreign/inactive evidence, altered claims or exhausted bounds.
    pub fn verify_inference_claim(
        &self,
        claim: &Claim,
        rule: &Rule,
        premise_budget: usize,
        max_depth: usize,
    ) -> Result<(), QueryError> {
        let entries = self.inference_premises(claim, premise_budget)?;
        let premises = entries
            .into_iter()
            .map(|entry| {
                let FactPayload::Edge(edge) = entry.version.payload else {
                    return Err(QueryError::InvalidLimit);
                };
                Ok(Premise {
                    generation: self.generation,
                    valid_from: entry.version.valid_from,
                    edge,
                })
            })
            .collect::<Result<Vec<_>, QueryError>>()?;
        syntaxmesh_inference::verify_witness(claim, rule, &premises, premise_budget, max_depth)
            .map_err(|error| QueryError::Context(error.to_string()))
    }
    /// Resolve ordered exact premise versions at this claim's retained selection.
    /// This is evidence resolution, not verification of an arbitrary claim's rule.
    ///
    /// # Errors
    /// Rejects foreign scope, missing/inactive premises and exhausted output bounds.
    pub fn inference_premises(
        &self,
        claim: &Claim,
        premise_budget: usize,
    ) -> Result<Vec<FactHistoryEntry>, QueryError> {
        if premise_budget == 0 || claim.premises.is_empty() {
            return Err(QueryError::InvalidLimit);
        }
        if claim.premises.len() > premise_budget {
            return Err(QueryError::Context(
                "inference premise budget exhausted".to_owned(),
            ));
        }
        let manifest = self.manifest()?;
        if claim.repository != manifest.repository
            || claim.worktree != manifest.worktree
            || claim.generation != self.generation
        {
            return Err(QueryError::Context(
                "foreign inference claim selection".to_owned(),
            ));
        }
        let sequence = self.store.generation_sequence(self.generation)?;
        let mut resolved = Vec::new();
        for reference in &claim.premises {
            let versions = self
                .store
                .fact_versions_changed_at(reference.fact, reference.valid_from)?;
            if versions.len() > 2 {
                return Err(QueryError::Context(
                    "malformed premise version lookup".to_owned(),
                ));
            }
            let mut matching = versions.into_iter().filter(|entry| {
                entry.fact == reference.fact && entry.version.valid_from == reference.valid_from
            });
            let mut entry = matching.next().ok_or_else(|| {
                QueryError::Context("missing inference premise version".to_owned())
            })?;
            if matching.next().is_some()
                || entry.valid_from_sequence
                    != self.store.generation_sequence(reference.valid_from)?
                || !matches!(&entry.version.payload, FactPayload::Edge(edge) if reference.fact == FactRef::Edge(edge.id))
                || entry.valid_until_sequence.is_some() != entry.version.valid_until.is_some()
                || entry
                    .valid_until_sequence
                    .is_some_and(|until| until <= entry.valid_from_sequence || until <= sequence)
            {
                return Err(QueryError::Context(
                    "inactive or malformed inference premise".to_owned(),
                ));
            }
            if let Some(until) = entry.version.valid_until
                && entry.valid_until_sequence != Some(self.store.generation_sequence(until)?)
            {
                return Err(QueryError::Context(
                    "inference premise closure generation disagrees with sequence".to_owned(),
                ));
            }
            entry.valid_until_sequence = None;
            entry.version.valid_until = None;
            super::history_page::validate_page(
                &FactHistoryPage {
                    items: vec![entry.clone()],
                    next_cursor: None,
                },
                self.generation,
                sequence,
                1,
                None,
                None,
            )?;
            resolved.push(entry);
        }
        Ok(resolved)
    }
}
