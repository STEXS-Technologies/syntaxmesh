use super::{PenelopeInferenceMaterializer, failure, journal};
use serde::{Deserialize, Serialize};
use std::path::Path;
use syntaxmesh_core::GenerationId;
use syntaxmesh_inference::{ClaimSelection, Contract, ContractLimits};
use syntaxmesh_inference_sqlite::{
    ContractHistoryRequest as SqliteContractHistoryRequest, ContractHistoryWatermark,
    InferenceMaterializer,
};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

const PREFIX: &str = "syntaxmesh.penelope.contract-history.v1/";

/// Durable Engine request for one complete contract-result generation.
pub struct ContractHistoryRequest<'contracts> {
    pub generation: GenerationId,
    pub contracts: &'contracts [Contract],
    pub limits: &'contracts ContractLimits,
    pub history_scan_budget: usize,
    pub output_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Request {
    selection: ClaimSelection,
    database: String,
    contracts: Vec<Contract>,
    contract_limit: usize,
    input_byte_limit: usize,
    edge_visit_limit: usize,
    result_limit: usize,
    history_scan_budget: usize,
    output_bytes: usize,
}

impl journal::Plan for Request {
    type Receipt = ContractHistoryWatermark;
    const DEFINITION: &'static str = "def_syntaxmesh_contract_history";
    const STEP: &'static str = "contract_history";

    fn key(&self) -> Result<String, WorkflowError> {
        let bytes = bincode::serialize(self).map_err(failure)?;
        Ok(format!(
            "{PREFIX}{}",
            hex::encode(penelope::ContentDigest::sha256(&bytes).0)
        ))
    }

    fn validate_receipt(&self, receipt: ContractHistoryWatermark) -> Result<(), WorkflowError> {
        if receipt.repository != self.selection.repository
            || receipt.worktree != self.selection.worktree
            || receipt.generation != self.selection.generation
            || receipt.result_count != self.contracts.len()
        {
            return Err(failure("contract history receipt scope/count mismatch"));
        }
        Ok(())
    }
}

impl<S: GraphStore + DurableRecordStore> PenelopeInferenceMaterializer<S> {
    /// Journal before evaluating or writing retained contract results.
    ///
    /// # Errors
    /// Invalid generations/limits, changed prepared requests, missing migrations,
    /// or downstream conflicts fail without replacing prior results.
    pub fn retain_contract_history(
        &mut self,
        database: impl AsRef<Path>,
        request: &ContractHistoryRequest<'_>,
    ) -> Result<ContractHistoryWatermark, WorkflowError> {
        if request.history_scan_budget == 0
            || request.output_bytes == 0
            || request.limits.contracts == 0
            || request.limits.input_bytes == 0
            || request.limits.edge_visits == 0
            || request.limits.results == 0
            || request.contracts.len() > request.limits.contracts
            || request.contracts.len() > request.limits.results
            || bincode::serialized_size(request.contracts).map_err(failure)?
                > u64::try_from(request.limits.input_bytes.min(journal::RULE_BYTES))
                    .map_err(failure)?
        {
            return Err(failure("positive contract history bounds required"));
        }
        let manifest = self.store.manifest(request.generation)?;
        let selection = ClaimSelection {
            repository: manifest.repository,
            worktree: manifest.worktree,
            generation: request.generation,
        };
        let path = database.as_ref();
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_err(failure)?.join(path)
        };
        let database_path = absolute
            .into_os_string()
            .into_string()
            .map_err(|_path| failure("inference database path must be UTF-8"))?;
        let mut contracts = request.contracts.to_vec();
        contracts
            .sort_by(|left, right| (&left.id, &left.version).cmp(&(&right.id, &right.version)));
        self.run_contract_history(&Request {
            selection,
            database: database_path,
            contracts,
            contract_limit: request.limits.contracts,
            input_byte_limit: request.limits.input_bytes,
            edge_visit_limit: request.limits.edge_visits,
            result_limit: request.limits.results,
            history_scan_budget: request.history_scan_budget,
            output_bytes: request.output_bytes,
        })
    }

    /// Recover prepared contract-history effects in bounded journal pages.
    ///
    /// # Errors
    /// Rejects malformed requests, unordered pages, missing source generations,
    /// or unavailable/mismatched downstream databases.
    pub fn recover_contract_history_pending(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = 0_usize;
        loop {
            let page = self
                .store
                .records_with_prefix_page(PREFIX, cursor.as_deref(), 32)?;
            if page.records.len() > 32 {
                return Err(failure("oversized contract history recovery page"));
            }
            let mut previous = cursor.clone();
            for (key, bytes) in &page.records {
                if !key.starts_with(PREFIX) || previous.as_ref().is_some_and(|last| key <= last) {
                    return Err(failure("unordered contract history recovery page"));
                }
                let record = journal::decode::<Request>(bytes)?;
                if journal::key(&record.request)? != *key {
                    return Err(failure("contract history journal key substitution"));
                }
                journal::validate(&record, &record.request)?;
                if record.receipt.is_none() {
                    self.run_contract_history(&record.request)?;
                    recovered = recovered
                        .checked_add(1)
                        .ok_or_else(|| failure("contract history recovery count overflow"))?;
                }
                previous = Some(key.clone());
            }
            let Some(next) = page.next_cursor else {
                return Ok(recovered);
            };
            if previous.as_deref() != Some(next.as_str())
                || cursor.as_ref().is_some_and(|last| &next <= last)
            {
                return Err(failure("nonadvancing contract history recovery cursor"));
            }
            cursor = Some(next);
        }
    }

    fn run_contract_history(
        &mut self,
        request: &Request,
    ) -> Result<ContractHistoryWatermark, WorkflowError> {
        let key = journal::key(request)?;
        let (record, bytes) = match self.store.read_record(&key)? {
            Some(bytes) => {
                let record = journal::decode(&bytes)?;
                journal::validate(&record, request)?;
                (record, bytes)
            }
            None => {
                let record = journal::prepare(request.clone())?;
                let bytes = journal::encode(&record)?;
                match self.store.compare_exchange_record(&key, None, &bytes) {
                    Ok(()) => (record, bytes),
                    Err(StoreError::RecordConflict(_)) => {
                        let raced = self.store.read_record(&key)?.ok_or_else(|| {
                            failure("contract history journal disappeared during prepare")
                        })?;
                        let raced_record = journal::decode(&raced)?;
                        journal::validate(&raced_record, request)?;
                        (raced_record, raced)
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };
        if let Some(receipt) = record.receipt {
            validate_effect(request, receipt)?;
            return Ok(receipt);
        }
        let manifest = self.store.manifest(request.selection.generation)?;
        if manifest.repository != request.selection.repository
            || manifest.worktree != request.selection.worktree
        {
            return Err(failure("prepared contract history source scope mismatch"));
        }
        let receipt = InferenceMaterializer::open(&request.database)
            .map_err(failure)?
            .retain_contract_history(
                &self.store,
                &SqliteContractHistoryRequest {
                    generation: request.selection.generation,
                    contracts: &request.contracts,
                    limits: &ContractLimits {
                        contracts: request.contract_limit,
                        input_bytes: request.input_byte_limit,
                        edge_visits: request.edge_visit_limit,
                        results: request.result_limit,
                    },
                    history_scan_budget: request.history_scan_budget,
                    output_bytes: request.output_bytes,
                },
            )
            .map_err(failure)?;
        let completed = journal::complete(record, receipt)?;
        let replacement = journal::encode(&completed)?;
        match self
            .store
            .compare_exchange_record(&key, Some(&bytes), &replacement)
        {
            Ok(()) => Ok(receipt),
            Err(StoreError::RecordConflict(_)) => {
                let current = self.store.read_record(&key)?.ok_or_else(|| {
                    failure("contract history journal disappeared during completion")
                })?;
                let current = journal::decode(&current)?;
                journal::validate(&current, request)?;
                if current.receipt != Some(receipt) {
                    return Err(failure("racing contract history completion mismatch"));
                }
                validate_effect(request, receipt)?;
                Ok(receipt)
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn validate_effect(
    request: &Request,
    receipt: ContractHistoryWatermark,
) -> Result<(), WorkflowError> {
    journal::Plan::validate_receipt(request, receipt)?;
    let materializer = InferenceMaterializer::open(&request.database).map_err(failure)?;
    let mut remaining = request.contracts.len();
    for contract in &request.contracts {
        if materializer
            .contract_result_at(
                request.selection.repository,
                request.selection.worktree,
                request.selection.generation,
                &contract.id,
                &contract.version,
            )
            .map_err(failure)?
            .is_none()
        {
            return Err(failure("contract history effect is incomplete"));
        }
        remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| failure("contract history receipt exceeds requested batch"))?;
    }
    if remaining != 0 {
        return Err(failure("contract history receipt count mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
