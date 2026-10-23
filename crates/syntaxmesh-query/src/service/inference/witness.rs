use crate::{Query, QueryError};
use std::collections::{BTreeMap, BTreeSet};
use syntaxmesh_api_model::{
    INFERENCE_EXPLANATION_SCHEMA_VERSION, InferenceClaim, InferenceDerivationClass,
    InferenceExplanation, InferencePremiseEvidence, InferenceSelection,
};
use syntaxmesh_core::{FactPayload, FactRef, FactVersionRef};
use syntaxmesh_inference::{Claim, DerivationClass, Premise, Rule};
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
        verify_loaded_witness(
            self.generation,
            claim,
            rule,
            &entries,
            premise_budget,
            max_depth,
        )
    }

    /// Return one complete, verified deterministic-rule explanation.
    ///
    /// The result contains the claim, exact ordered premise versions, retained
    /// edges, and producer provenance. It is not a recursive derivation DAG.
    ///
    /// # Errors
    /// Rejects invalid/foreign claims, inactive evidence, missing provenance,
    /// and exhausted premise, depth, or serialized-output bounds.
    pub fn explain_inference_claim(
        &self,
        claim: &Claim,
        rule: &Rule,
        premise_budget: usize,
        max_depth: usize,
        output_byte_budget: usize,
    ) -> Result<InferenceExplanation, QueryError> {
        if output_byte_budget == 0 {
            return Err(QueryError::InvalidLimit);
        }
        let entries = self.inference_premises(claim, premise_budget)?;
        verify_loaded_witness(
            self.generation,
            claim,
            rule,
            &entries,
            premise_budget,
            max_depth,
        )?;

        let provenance_ids = entries
            .iter()
            .map(|entry| match &entry.version.payload {
                FactPayload::Edge(edge) => Ok(edge.provenance),
                FactPayload::File(_) | FactPayload::Provenance(_) | FactPayload::Node(_) => {
                    Err(QueryError::InvalidLimit)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let requested = provenance_ids.iter().copied().collect::<BTreeSet<_>>();
        let records = self
            .store
            .provenance_for_ids(self.generation, &provenance_ids)?;
        if records.len() != requested.len() {
            return Err(QueryError::Context(
                "inference explanation is missing premise provenance".to_owned(),
            ));
        }
        let provenance = records
            .into_iter()
            .map(|record| (record.id, record))
            .collect::<BTreeMap<_, _>>();
        if provenance.len() != requested.len() {
            return Err(QueryError::Context(
                "inference explanation has duplicate premise provenance".to_owned(),
            ));
        }
        let mut evidence = Vec::with_capacity(entries.len());
        for entry in entries {
            let FactPayload::Edge(edge) = entry.version.payload else {
                return Err(QueryError::InvalidLimit);
            };
            let producer = provenance.get(&edge.provenance).ok_or_else(|| {
                QueryError::Context("inference explanation provenance identity mismatch".to_owned())
            })?;
            evidence.push(InferencePremiseEvidence {
                reference: FactVersionRef {
                    fact: entry.fact,
                    valid_from: entry.version.valid_from,
                },
                edge,
                provenance: producer.clone(),
            });
        }
        let manifest = self.manifest()?;
        let explanation = InferenceExplanation {
            schema_version: INFERENCE_EXPLANATION_SCHEMA_VERSION,
            selection: InferenceSelection {
                repository: manifest.repository,
                worktree: manifest.worktree,
                generation: self.generation,
            },
            claim: api_claim(claim),
            premises: evidence,
        };
        let bytes = serde_json::to_vec(&explanation)
            .map_err(|error| QueryError::Serialization(error.to_string()))?;
        if bytes.len() > output_byte_budget {
            return Err(QueryError::Context(
                "inference explanation output byte budget exhausted".to_owned(),
            ));
        }
        Ok(explanation)
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

fn verify_loaded_witness(
    generation: syntaxmesh_core::GenerationId,
    claim: &Claim,
    rule: &Rule,
    entries: &[FactHistoryEntry],
    premise_budget: usize,
    max_depth: usize,
) -> Result<(), QueryError> {
    let premises = entries
        .iter()
        .map(|entry| {
            let FactPayload::Edge(edge) = &entry.version.payload else {
                return Err(QueryError::InvalidLimit);
            };
            Ok(Premise {
                generation,
                valid_from: entry.version.valid_from,
                edge: edge.clone(),
            })
        })
        .collect::<Result<Vec<_>, QueryError>>()?;
    syntaxmesh_inference::verify_witness(claim, rule, &premises, premise_budget, max_depth)
        .map_err(|error| QueryError::Context(error.to_string()))
}

fn api_claim(claim: &Claim) -> InferenceClaim {
    InferenceClaim {
        id: claim.id,
        derivation_class: match claim.derivation_class {
            DerivationClass::DeterministicRule => InferenceDerivationClass::DeterministicRule,
            DerivationClass::HeuristicRule => InferenceDerivationClass::HeuristicRule,
            DerivationClass::ModelInference => InferenceDerivationClass::ModelInference,
            DerivationClass::UserDeclared => InferenceDerivationClass::UserDeclared,
            DerivationClass::RuntimeCorrelation => InferenceDerivationClass::RuntimeCorrelation,
        },
        repository: claim.repository,
        worktree: claim.worktree,
        generation: claim.generation,
        source: claim.source,
        target: claim.target,
        relation: claim.relation.clone(),
        rule_id: claim.rule_id.clone(),
        rule_version: claim.rule_version.clone(),
        ontology_versions: claim.ontology_versions.clone(),
        configuration_fingerprint: claim.configuration_fingerprint,
        premises: claim.premises.clone(),
    }
}
