use super::{EngineError, SyntaxMeshEngine};
use std::path::Path;
use syntaxmesh_inference::{OntologyDefinition, RegisteredInferenceAdmission};
use syntaxmesh_inference_sqlite::{
    ContractHistoryPage, ContractHistoryQuery, ContractHistoryWatermark, InferenceMaterializer,
    RebuildRequest, RetainedContractEvaluation, StreamCheckpoint, StreamOrigin, StreamSyncRequest,
};
use syntaxmesh_integration_penelope::{
    ContractHistoryRequest, PenelopeInferenceMaterializer, PenelopeInferenceSetup,
    PenelopeInferenceStream,
};
use syntaxmesh_language_sdk::LanguageExtractor;
use syntaxmesh_store::{DurableRecordStore, GraphStore};
use syntaxmesh_workflow::WorkflowError;

/// Effects recovered in dependency order; not canonical publication counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InferenceRecovery {
    pub contract_history: usize,
    pub baselines: usize,
    pub setups: usize,
    pub stream_effects: usize,
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
        request: &ContractHistoryRequest<'_>,
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
        Ok(PenelopeInferenceMaterializer::new(self.indexer.store_mut())
            .retain_contract_history(database, request)?)
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

#[cfg(test)]
mod tests;

mod read;
