use super::{PenelopeInferenceStream, failure, journal};
use serde::{Deserialize, Serialize};
use std::path::Path;
use syntaxmesh_core::StableId;
use syntaxmesh_inference_sqlite::{InferenceMaterializer, StreamOrigin, Watermark};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

const PREFIX: &str = "syntaxmesh.penelope.inference-stream-begin.v1/";

/// Initialization from an accepted complete baseline; migrations remain explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamBeginRequest {
    pub baseline: Watermark,
    pub claim_budget: usize,
    pub byte_budget: usize,
    pub premise_budget: usize,
    pub page_budget: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Request {
    baseline: Watermark,
    database: String,
    claim_budget: usize,
    byte_budget: usize,
    premise_budget: usize,
    page_budget: usize,
}

impl journal::Plan for Request {
    type Receipt = StreamOrigin;
    const DEFINITION: &'static str = "def_syntaxmesh_inference_stream_begin";
    const STEP: &'static str = "inference_stream_begin";

    fn key(&self) -> Result<String, WorkflowError> {
        let encoded = bincode::serialize(&(self.baseline, &self.database)).map_err(failure)?;
        Ok(format!(
            "{PREFIX}{}",
            hex::encode(penelope::ContentDigest::sha256(&encoded).0)
        ))
    }

    fn validate_receipt(&self, receipt: StreamOrigin) -> Result<(), WorkflowError> {
        if receipt.baseline != self.baseline
            || receipt.stream
                != StableId::derive("inference-stream-v1", &[&self.baseline.operation.0])
            || self.baseline.claim_count > self.claim_budget
        {
            return Err(failure("stream initialization receipt substitution"));
        }
        Ok(())
    }
}

impl<S: GraphStore + DurableRecordStore> PenelopeInferenceStream<S> {
    /// Journal initialization before publishing its baseline active root.
    ///
    /// # Errors
    /// Rejects invalid bounds, changed requests, corrupt baselines and failed effects.
    pub fn begin(
        &mut self,
        database: impl AsRef<Path>,
        request: &StreamBeginRequest,
    ) -> Result<StreamOrigin, WorkflowError> {
        if request.claim_budget == 0
            || request.byte_budget == 0
            || request.premise_budget == 0
            || request.page_budget == 0
        {
            return Err(failure("positive stream initialization admission required"));
        }
        let path = database.as_ref();
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_err(failure)?.join(path)
        };
        let database_path = absolute
            .into_os_string()
            .into_string()
            .map_err(|_path| failure("stream initialization database path must be UTF-8"))?;
        self.run_begin(&Request {
            baseline: request.baseline,
            database: database_path,
            claim_budget: request.claim_budget,
            byte_budget: request.byte_budget,
            premise_budget: request.premise_budget,
            page_budget: request.page_budget,
        })
    }

    pub(super) fn recover_begins(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = 0_usize;
        loop {
            let page = self
                .store
                .records_with_prefix_page(PREFIX, cursor.as_deref(), 32)?;
            if page.records.len() > 32 {
                return Err(failure("oversized stream initialization recovery page"));
            }
            let mut previous = cursor.clone();
            for (key, bytes) in &page.records {
                if !key.starts_with(PREFIX) || previous.as_ref().is_some_and(|last| key <= last) {
                    return Err(failure("unordered stream initialization recovery key"));
                }
                let record = journal::decode::<Request>(bytes)?;
                if journal::key(&record.request)? != *key {
                    return Err(failure("stream initialization key substitution"));
                }
                journal::validate(&record, &record.request)?;
                if record.receipt.is_none() {
                    self.run_begin(&record.request)?;
                    recovered = recovered
                        .checked_add(1)
                        .ok_or_else(|| failure("stream initialization recovery count overflow"))?;
                }
                previous = Some(key.clone());
            }
            let Some(next) = page.next_cursor else {
                return Ok(recovered);
            };
            if previous.as_deref() != Some(next.as_str())
                || cursor.as_ref().is_some_and(|last| &next <= last)
            {
                return Err(failure(
                    "nonadvancing stream initialization recovery cursor",
                ));
            }
            cursor = Some(next);
        }
    }

    fn run_begin(&mut self, request: &Request) -> Result<StreamOrigin, WorkflowError> {
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
                            failure("stream initialization journal disappeared during prepare")
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
        let receipt = publish(request)?;
        let completed = journal::complete(record, receipt)?;
        let replacement = journal::encode(&completed)?;
        match self
            .store
            .compare_exchange_record(&key, Some(&bytes), &replacement)
        {
            Ok(()) => Ok(receipt),
            Err(StoreError::RecordConflict(_)) => {
                let current = self.store.read_record(&key)?.ok_or_else(|| {
                    failure("stream initialization journal disappeared during completion")
                })?;
                let current = journal::decode(&current)?;
                journal::validate(&current, request)?;
                if current.receipt != Some(receipt) {
                    return Err(failure("racing stream initialization completion mismatch"));
                }
                validate_effect(request, receipt)?;
                Ok(receipt)
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn publish(request: &Request) -> Result<StreamOrigin, WorkflowError> {
    InferenceMaterializer::open(&request.database)
        .map_err(failure)?
        .begin_stream(
            request.baseline,
            request.claim_budget,
            request.byte_budget,
            request.premise_budget,
            request.page_budget,
        )
        .map_err(failure)
}

fn validate_effect(request: &Request, receipt: StreamOrigin) -> Result<(), WorkflowError> {
    <Request as journal::Plan>::validate_receipt(request, receipt)?;
    let materializer = InferenceMaterializer::open(&request.database).map_err(failure)?;
    let checkpoint = materializer
        .stream_checkpoint(receipt, request.baseline.selection.generation)
        .map_err(failure)?
        .ok_or_else(|| failure("completed initialization checkpoint missing"))?;
    if checkpoint.previous.is_some()
        || checkpoint.sequence != request.baseline.generation_sequence
        || checkpoint.added_claims != request.baseline.claim_count
        || checkpoint.active_claims != request.baseline.claim_count
        || checkpoint.affected_claims != 0
        || checkpoint.closure_reasons != 0
    {
        return Err(failure("completed initialization checkpoint mismatch"));
    }
    drop(materializer);
    if publish(request)? != receipt {
        return Err(failure("completed initialization effect mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
