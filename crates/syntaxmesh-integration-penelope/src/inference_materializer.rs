use serde::{Deserialize, Serialize};
use std::path::Path;
use syntaxmesh_core::StableId;
use syntaxmesh_inference::{ClaimSelection, Limits, Rule, materialization_identity};
use syntaxmesh_inference_sqlite::{InferenceMaterializer, RebuildRequest, Watermark};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

mod contract_history;
mod journal;
mod lifecycle;
mod registered;
mod setup;
mod stream;
pub use contract_history::ContractHistoryRequest;
pub use lifecycle::PenelopeInferenceLifecycle;
pub use registered::RegisteredInferenceAdmission;
pub use setup::PenelopeInferenceSetup;
pub use stream::{PenelopeInferenceStream, StreamBeginRequest};

const PREFIX: &str = "syntaxmesh.penelope.inference-materializer.v1/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Request {
    selection: ClaimSelection,
    operation: StableId,
    database: String,
    rules: Vec<Rule>,
    limits: Limits,
    history_scan_budget: usize,
    rule_budget: usize,
    output_bytes: usize,
    premise_budget: usize,
}

impl journal::Plan for Request {
    type Receipt = Watermark;
    const DEFINITION: &'static str = "def_syntaxmesh_inference_materializer";
    const STEP: &'static str = "inference";

    fn key(&self) -> Result<String, WorkflowError> {
        let operation = materialization_identity(
            self.selection,
            &self.rules,
            self.rule_budget,
            self.limits.input_bytes.min(journal::RULE_BYTES),
        )
        .map_err(failure)?;
        if operation != self.operation {
            return Err(failure("request semantic operation mismatch"));
        }
        let bytes = bincode::serialize(&(operation, &self.database)).map_err(failure)?;
        Ok(format!(
            "{PREFIX}{}",
            hex::encode(penelope::ContentDigest::sha256(&bytes).0)
        ))
    }

    fn validate_receipt(&self, receipt: Watermark) -> Result<(), WorkflowError> {
        if receipt.operation != self.operation || receipt.selection != self.selection {
            return Err(failure("inference journal receipt scope mismatch"));
        }
        Ok(())
    }
}

/// Explicit Penelope journaling for downstream complete inference rebuilds.
pub struct PenelopeInferenceMaterializer<S> {
    store: S,
}

impl<S: GraphStore + DurableRecordStore> PenelopeInferenceMaterializer<S> {
    #[must_use]
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Journal a pinned rebuild before any downstream effect. Migrate explicitly first.
    ///
    /// # Errors
    /// Failed effects stay prepared; changed request controls or corrupt journals reject.
    pub fn rebuild(
        &mut self,
        database: impl AsRef<Path>,
        request: &RebuildRequest<'_>,
    ) -> Result<Watermark, WorkflowError> {
        if request.history_scan_budget == 0
            || request.output_bytes == 0
            || request.premise_budget == 0
            || request.limits.facts == 0
            || request.limits.edge_visits == 0
            || request.limits.depth == 0
            || request.limits.claims == 0
        {
            return Err(failure("positive inference execution controls required"));
        }
        let manifest = self.store.manifest(request.generation)?;
        let selection = ClaimSelection {
            repository: manifest.repository,
            worktree: manifest.worktree,
            generation: request.generation,
        };
        let operation = materialization_identity(
            selection,
            request.rules,
            request.rule_budget,
            request.limits.input_bytes.min(journal::RULE_BYTES),
        )
        .map_err(failure)?;
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
        let mut rules = request.rules.to_vec();
        rules.sort_by(|left, right| (&left.id, &left.version).cmp(&(&right.id, &right.version)));
        self.run(&Request {
            selection,
            operation,
            database: database_path,
            rules,
            limits: *request.limits,
            history_scan_budget: request.history_scan_budget,
            rule_budget: request.rule_budget,
            output_bytes: request.output_bytes,
            premise_budget: request.premise_budget,
        })
    }

    /// Replay and recover prepared requests through bounded durable-record pages.
    ///
    /// # Errors
    /// Rejects malformed records/pages or unavailable effects/source generations.
    pub fn recover_pending(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = self.recover_contract_history_pending()?;
        recovered = recovered
            .checked_add(self.recover_registered_pending()?)
            .ok_or_else(|| failure("inference recovery count overflow"))?;
        loop {
            let page = self
                .store
                .records_with_prefix_page(PREFIX, cursor.as_deref(), 32)?;
            if page.records.len() > 32 {
                return Err(failure("oversized inference recovery page"));
            }
            let mut previous = cursor.clone();
            for (key, bytes) in &page.records {
                if !key.starts_with(PREFIX) || previous.as_ref().is_some_and(|last| key <= last) {
                    return Err(failure("unordered or foreign inference recovery key"));
                }
                let record = journal::decode::<Request>(bytes)?;
                if journal::key(&record.request)? != *key {
                    return Err(failure("inference journal key substitution"));
                }
                journal::validate(&record, &record.request)?;
                if record.receipt.is_none() {
                    self.run(&record.request)?;
                    recovered = recovered
                        .checked_add(1)
                        .ok_or_else(|| failure("inference recovery count overflow"))?;
                }
                previous = Some(key.clone());
            }
            let Some(next) = page.next_cursor else {
                return Ok(recovered);
            };
            if previous.as_deref() != Some(next.as_str())
                || cursor.as_ref().is_some_and(|last| &next <= last)
            {
                return Err(failure("nonadvancing inference recovery cursor"));
            }
            cursor = Some(next);
        }
    }

    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }

    fn run(&mut self, request: &Request) -> Result<Watermark, WorkflowError> {
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
                        let raced_bytes = self.store.read_record(&key)?.ok_or_else(|| {
                            failure("inference journal disappeared during prepare")
                        })?;
                        let raced_record = journal::decode(&raced_bytes)?;
                        journal::validate(&raced_record, request)?;
                        (raced_record, raced_bytes)
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
            return Err(failure("prepared inference source scope mismatch"));
        }
        let receipt = InferenceMaterializer::open(&request.database)
            .map_err(failure)?
            .rebuild(
                &self.store,
                &RebuildRequest {
                    generation: request.selection.generation,
                    rules: &request.rules,
                    limits: &request.limits,
                    history_scan_budget: request.history_scan_budget,
                    rule_budget: request.rule_budget,
                    output_bytes: request.output_bytes,
                    premise_budget: request.premise_budget,
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
                let current = self
                    .store
                    .read_record(&key)?
                    .ok_or_else(|| failure("inference journal disappeared during completion"))?;
                let current = journal::decode(&current)?;
                journal::validate(&current, request)?;
                if current.receipt != Some(receipt) {
                    return Err(failure("racing inference completion mismatch"));
                }
                validate_effect(request, receipt)?;
                Ok(receipt)
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn validate_effect(request: &Request, receipt: Watermark) -> Result<(), WorkflowError> {
    if receipt.operation != request.operation || receipt.selection != request.selection {
        return Err(failure("inference receipt operation mismatch"));
    }
    InferenceMaterializer::open(&request.database)
        .map_err(failure)?
        .claims(receipt, request.limits.claims, request.output_bytes)
        .map_err(failure)?;
    Ok(())
}

fn failure(error: impl std::fmt::Display) -> WorkflowError {
    WorkflowError::Verification(format!("inference materializer: {error}"))
}

#[cfg(test)]
mod tests;
