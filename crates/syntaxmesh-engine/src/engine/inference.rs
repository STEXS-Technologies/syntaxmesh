use super::{EngineError, SyntaxMeshEngine};
use std::path::Path;
use syntaxmesh_inference::{ContractLimits, OntologyDefinition, RegisteredInferenceAdmission};
use syntaxmesh_inference_sqlite::{
    ContractHistoryPage, ContractHistoryQuery, ContractHistoryWatermark, InferenceMaterializer,
    RebuildRequest, RetainedContractEvaluation, RetainedContractSet, StreamCheckpoint,
    StreamOrigin, StreamSyncRequest,
};
use syntaxmesh_integration_penelope::{
    ContractHistoryRequest as PenelopeContractHistoryRequest, PenelopeInferenceMaterializer,
    PenelopeInferenceSetup, PenelopeInferenceStream,
};
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

/// Effects recovered in dependency order; not canonical publication counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InferenceRecovery {
    pub contract_history: usize,
    pub baselines: usize,
    pub setups: usize,
    pub stream_effects: usize,
}

/// Bounds for catch-up of the latest retained contract set over later generations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractHistoryAdvanceRequest {
    pub through_generation: syntaxmesh_core::GenerationId,
    pub generation_budget: usize,
    pub history_scan_budget: usize,
    pub output_bytes_per_generation: usize,
    pub limits: ContractLimits,
}

/// Summary of an explicit, bounded contract-history catch-up operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractHistoryAdvanceReceipt {
    pub starting_generation: syntaxmesh_core::GenerationId,
    pub through_generation: syntaxmesh_core::GenerationId,
    pub starting_sequence: u64,
    pub through_sequence: u64,
    pub generations_advanced: usize,
    pub contract_count: usize,
    pub last_watermark: ContractHistoryWatermark,
}

impl<S: GraphStore + DurableRecordStore, E: LanguageExtractor> SyntaxMeshEngine<S, E> {
    /// Recover baselines, parent setups, then stream initialization and successors.
    /// Database migrations remain explicit host operations.
    /// Recovery is store-wide for already journaled requests; new effect requests
    /// below are restricted to this engine's repository/worktree.
    ///
    /// # Errors
    /// Rejects malformed jobs and missing/corrupt source or downstream effects.
    pub fn recover_inference(&mut self) -> Result<InferenceRecovery, EngineError> {
        let contract_history = PenelopeInferenceMaterializer::new(self.indexer.store_mut())
            .recover_contract_history_pending()?;
        let baselines =
            PenelopeInferenceMaterializer::new(self.indexer.store_mut()).recover_pending()?;
        let setups = PenelopeInferenceSetup::new(self.indexer.store_mut()).recover_pending()?;
        let stream_effects =
            PenelopeInferenceStream::new(self.indexer.store_mut()).recover_pending()?;
        Ok(InferenceRecovery {
            contract_history,
            baselines,
            setups,
            stream_effects,
        })
    }

    /// Evaluate and retain one generation-pinned contract set through Penelope.
    /// Call explicit downstream inference migration first; recovery is invoked
    /// before any new request is accepted.
    ///
    /// # Errors
    /// Rejects a foreign generation, invalid bounds, changed journal inputs,
    /// or failed downstream materialization.
    pub fn retain_contract_history(
        &mut self,
        database: impl AsRef<Path>,
        request: &PenelopeContractHistoryRequest<'_>,
    ) -> Result<ContractHistoryWatermark, EngineError> {
        let manifest = self.indexer.store().manifest(request.generation)?;
        if manifest.repository != self.repository || manifest.worktree != self.worktree {
            return Err(syntaxmesh_workflow::WorkflowError::Store(
                syntaxmesh_store::StoreError::InvalidDelta(
                    "contract history generation outside engine scope".to_owned(),
                ),
            )
            .into());
        }
        self.recover_pending_workflows()?;
        Ok(
            PenelopeInferenceMaterializer::new(self.indexer.store_mut()).retain_contract_history(
                database,
                &PenelopeContractHistoryRequest {
                    generation: request.generation,
                    contracts: request.contracts,
                    limits: request.limits,
                    history_scan_budget: request.history_scan_budget,
                    output_bytes: request.output_bytes,
                },
            )?,
        )
    }

    /// Catch the latest complete retained contract set up through one pinned
    /// generation, journaling each immediate successor through Penelope.
    ///
    /// The latest retained declarations are reused; this operation does not
    /// accept or infer replacements. It preflights the exact generation
    /// distance before any new result is published, then leaves a durable
    /// complete prefix if an individual generation fails.
    ///
    /// # Errors
    /// Rejects missing/corrupt baselines, reverse or foreign ranges, invalid
    /// bounds, noncontiguous history, or failed Penelope/SQLite effects.
    pub fn advance_contract_history(
        &mut self,
        database: impl AsRef<Path>,
        request: ContractHistoryAdvanceRequest,
    ) -> Result<ContractHistoryAdvanceReceipt, EngineError> {
        if request.generation_budget == 0
            || request.generation_budget > 10_000
            || request.history_scan_budget == 0
            || request.output_bytes_per_generation == 0
            || request.limits.contracts == 0
            || request.limits.input_bytes == 0
            || request.limits.edge_visits == 0
            || request.limits.results == 0
        {
            return Err(invalid_contract_history_advance(
                "positive bounded contract history limits required",
            ));
        }
        let through_manifest = self.indexer.store().manifest(request.through_generation)?;
        if through_manifest.repository != self.repository
            || through_manifest.worktree != self.worktree
        {
            return Err(invalid_contract_history_advance(
                "contract history catch-up target outside engine scope",
            ));
        }
        self.recover_pending_workflows()?;
        let _ = PenelopeInferenceMaterializer::new(self.indexer.store_mut())
            .recover_contract_history_pending()?;
        let database = database.as_ref();
        let materializer = InferenceMaterializer::open(database)
            .map_err(|error| WorkflowError::Verification(error.to_string()))?;
        let retained = materializer
            .latest_contract_set(self.repository, self.worktree)
            .map_err(|error| WorkflowError::Verification(error.to_string()))?
            .ok_or_else(|| {
                invalid_contract_history_advance(
                    "contract history catch-up requires an existing retained baseline",
                )
            })?;
        drop(materializer);
        let RetainedContractSet {
            watermark: baseline,
            contracts,
        } = retained;
        let through_sequence = self
            .indexer
            .store()
            .generation_sequence(request.through_generation)?;
        if baseline.generation_sequence > through_sequence {
            return Err(invalid_contract_history_advance(
                "contract history catch-up target precedes latest retained baseline",
            ));
        }
        if contracts.is_empty()
            || contracts.len() > request.limits.contracts
            || contracts.len() > request.limits.results
        {
            return Err(invalid_contract_history_advance(
                "retained contract set exceeds catch-up limits or is empty",
            ));
        }
        let generation_distance = through_sequence
            .checked_sub(baseline.generation_sequence)
            .ok_or_else(|| {
                invalid_contract_history_advance("contract generation range reversed")
            })?;
        let generation_distance = usize::try_from(generation_distance)
            .map_err(|error| WorkflowError::Verification(error.to_string()))?;
        if generation_distance > request.generation_budget {
            return Err(invalid_contract_history_advance(
                "contract history generation budget exhausted before publication",
            ));
        }
        let mut previous_generation = baseline.generation;
        let mut previous_sequence = baseline.generation_sequence;
        let mut cursor = None;
        let mut last_watermark = baseline;
        for _ in 0..generation_distance {
            let page = self.indexer.store().changes_between_page(
                baseline.generation,
                request.through_generation,
                cursor,
                1,
            )?;
            let change = page.items.first().ok_or_else(|| {
                invalid_contract_history_advance("retained generation transition is missing")
            })?;
            let expected_sequence = previous_sequence
                .checked_add(1)
                .ok_or_else(|| invalid_contract_history_advance("generation sequence overflow"))?;
            if page.items.len() != 1
                || change.sequence != expected_sequence
                || change.delta.expected_base != Some(previous_generation)
                || change.manifest.parent != Some(previous_generation)
                || change.delta.next_generation != change.manifest.generation
                || change.manifest.repository != self.repository
                || change.manifest.worktree != self.worktree
                || self
                    .indexer
                    .store()
                    .generation_sequence(change.manifest.generation)?
                    != change.sequence
            {
                return Err(invalid_contract_history_advance(
                    "contract history range contains a noncontiguous or foreign transition",
                ));
            }
            last_watermark = PenelopeInferenceMaterializer::new(self.indexer.store_mut())
                .retain_contract_history(
                    database,
                    &PenelopeContractHistoryRequest {
                        generation: change.manifest.generation,
                        contracts: &contracts,
                        limits: &request.limits,
                        history_scan_budget: request.history_scan_budget,
                        output_bytes: request.output_bytes_per_generation,
                    },
                )?;
            previous_generation = change.manifest.generation;
            previous_sequence = change.sequence;
            cursor = page.next_cursor;
        }
        if previous_generation != request.through_generation
            || previous_sequence != through_sequence
        {
            return Err(invalid_contract_history_advance(
                "contract history catch-up did not reach its pinned target",
            ));
        }
        Ok(ContractHistoryAdvanceReceipt {
            starting_generation: baseline.generation,
            through_generation: request.through_generation,
            starting_sequence: baseline.generation_sequence,
            through_sequence,
            generations_advanced: generation_distance,
            contract_count: contracts.len(),
            last_watermark,
        })
    }

    /// Read one already-materialized exact contract result without replaying graph history.
    ///
    /// # Errors
    /// Rejects an unmigrated downstream database or corrupt retained payload.
    pub fn contract_result_at(
        &self,
        database: impl AsRef<Path>,
        repository: syntaxmesh_core::RepositoryId,
        worktree: syntaxmesh_core::WorktreeId,
        generation: syntaxmesh_core::GenerationId,
        contract_id: &str,
        contract_version: &str,
    ) -> Result<Option<RetainedContractEvaluation>, EngineError> {
        Ok(InferenceMaterializer::open(database)
            .map_err(|error| WorkflowError::Verification(error.to_string()))?
            .contract_result_at(
                repository,
                worktree,
                generation,
                contract_id,
                contract_version,
            )
            .map_err(|error| WorkflowError::Verification(error.to_string()))?)
    }

    /// Read a bounded indexed chronological page of materialized contract results.
    ///
    /// # Errors
    /// Rejects invalid/cross-contract cursors, page bounds, unmigrated databases,
    /// or corrupt retained payloads.
    pub fn contract_history(
        &self,
        database: impl AsRef<Path>,
        query: &ContractHistoryQuery,
    ) -> Result<ContractHistoryPage, EngineError> {
        Ok(InferenceMaterializer::open(database)
            .map_err(|error| WorkflowError::Verification(error.to_string()))?
            .contract_history(query)
            .map_err(|error| WorkflowError::Verification(error.to_string()))?)
    }

    /// Publish a complete baseline and initialize its temporal stream via Penelope.
    /// Call recovery first; migrate the downstream database explicitly.
    ///
    /// # Errors
    /// Rejects foreign engine scope, invalid bounds, changed requests and failed effects.
    pub fn initialize_inference_stream(
        &mut self,
        database: impl AsRef<Path>,
        request: &RebuildRequest<'_>,
        page_budget: usize,
    ) -> Result<StreamOrigin, EngineError> {
        let manifest = self.indexer.store().manifest(request.generation)?;
        if manifest.repository != self.repository || manifest.worktree != self.worktree {
            return Err(syntaxmesh_workflow::WorkflowError::Store(
                syntaxmesh_store::StoreError::InvalidDelta(
                    "inference initialization outside engine scope".to_owned(),
                ),
            )
            .into());
        }
        if page_budget == 0 {
            return Err(syntaxmesh_workflow::WorkflowError::Store(
                syntaxmesh_store::StoreError::InvalidDelta(
                    "positive inference page budget required".to_owned(),
                ),
            )
            .into());
        }
        Ok(
            PenelopeInferenceSetup::new(self.indexer.store_mut()).initialize(
                database,
                request,
                page_budget,
            )?,
        )
    }

    /// Publish an ontology-validated baseline and initialize its registered stream.
    /// Call recovery first; migrate the downstream database explicitly.
    ///
    /// # Errors
    /// Rejects foreign engine scope, invalid bounds, changed requests and failed effects.
    pub fn initialize_registered_inference_stream(
        &mut self,
        database: impl AsRef<Path>,
        request: &RebuildRequest<'_>,
        definitions: &[OntologyDefinition],
        admission: RegisteredInferenceAdmission,
        page_budget: usize,
    ) -> Result<StreamOrigin, EngineError> {
        let manifest = self.indexer.store().manifest(request.generation)?;
        if manifest.repository != self.repository || manifest.worktree != self.worktree {
            return Err(syntaxmesh_workflow::WorkflowError::Store(
                syntaxmesh_store::StoreError::InvalidDelta(
                    "registered inference initialization outside engine scope".to_owned(),
                ),
            )
            .into());
        }
        if page_budget == 0 {
            return Err(syntaxmesh_workflow::WorkflowError::Store(
                syntaxmesh_store::StoreError::InvalidDelta(
                    "positive registered inference page budget required".to_owned(),
                ),
            )
            .into());
        }
        Ok(
            PenelopeInferenceSetup::new(self.indexer.store_mut()).initialize_registered(
                database,
                request,
                definitions,
                admission,
                page_budget,
            )?,
        )
    }

    /// Synchronize one immediate accepted successor through Penelope.
    ///
    /// # Errors
    /// Rejects foreign scope, skipped transitions, invalid controls and failed effects.
    pub fn synchronize_inference_stream(
        &mut self,
        database: impl AsRef<Path>,
        request: &StreamSyncRequest<'_>,
    ) -> Result<StreamCheckpoint, EngineError> {
        let selection = request.origin.baseline.selection;
        let manifest = self.indexer.store().manifest(request.through)?;
        if selection.repository != self.repository
            || selection.worktree != self.worktree
            || manifest.repository != self.repository
            || manifest.worktree != self.worktree
        {
            return Err(syntaxmesh_workflow::WorkflowError::Store(
                syntaxmesh_store::StoreError::InvalidDelta(
                    "inference synchronization outside engine scope".to_owned(),
                ),
            )
            .into());
        }
        Ok(PenelopeInferenceStream::new(self.indexer.store_mut()).sync(database, request)?)
    }

    /// Synchronize one registered immediate successor through Penelope.
    /// Ontology definitions are durably retained in the versioned request.
    ///
    /// # Errors
    /// Rejects foreign scope, changed ontology identity, invalid facts, membership
    /// retractions, skipped transitions and failed effects.
    pub fn synchronize_registered_inference_stream(
        &mut self,
        database: impl AsRef<Path>,
        request: &StreamSyncRequest<'_>,
        definitions: &[OntologyDefinition],
        admission: RegisteredInferenceAdmission,
    ) -> Result<StreamCheckpoint, EngineError> {
        let selection = request.origin.baseline.selection;
        let manifest = self.indexer.store().manifest(request.through)?;
        if selection.repository != self.repository
            || selection.worktree != self.worktree
            || manifest.repository != self.repository
            || manifest.worktree != self.worktree
        {
            return Err(syntaxmesh_workflow::WorkflowError::Store(
                syntaxmesh_store::StoreError::InvalidDelta(
                    "registered inference synchronization outside engine scope".to_owned(),
                ),
            )
            .into());
        }
        Ok(
            PenelopeInferenceStream::new(self.indexer.store_mut()).sync_registered(
                database,
                request,
                definitions,
                admission,
            )?,
        )
    }
}

fn invalid_contract_history_advance(message: &str) -> EngineError {
    WorkflowError::Store(StoreError::InvalidDelta(message.to_owned())).into()
}

#[cfg(test)]
mod tests;

mod read;
