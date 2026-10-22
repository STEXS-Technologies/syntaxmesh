use super::{failure, journal};
use serde::{Deserialize, Serialize};
use std::path::Path;
use syntaxmesh_core::GenerationId;
use syntaxmesh_inference::InvalidationLimits;
use syntaxmesh_inference_sqlite::{
    InferenceMaterializer, LifecycleReceipt, LifecycleRequest, Watermark,
};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

const PREFIX: &str = "syntaxmesh.penelope.inference-lifecycle.v1/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Request {
    origin: Watermark,
    through: GenerationId,
    database: String,
    limits: InvalidationLimits,
    evidence_budget: usize,
    claim_bytes: usize,
}

impl journal::Plan for Request {
    type Receipt = LifecycleReceipt;
    const DEFINITION: &'static str = "def_syntaxmesh_inference_lifecycle";
    const STEP: &'static str = "inference_lifecycle";

    fn key(&self) -> Result<String, WorkflowError> {
        let encoded = bincode::serialize(&(self.origin.operation, self.through, &self.database))
            .map_err(failure)?;
        Ok(format!(
            "{PREFIX}{}",
            hex::encode(penelope::ContentDigest::sha256(&encoded).0)
        ))
    }

    fn validate_receipt(&self, receipt: LifecycleReceipt) -> Result<(), WorkflowError> {
        if receipt.operation != self.origin.operation
            || receipt.checked_through.generation != self.through
            || receipt.affected_claims > self.limits.claims
            || receipt.closure_reasons > self.evidence_budget
        {
            return Err(failure("lifecycle receipt scope or output bound mismatch"));
        }
        Ok(())
    }
}

/// Explicit recoverable direct premise lifecycle effects; no scheduler or rule replay.
pub struct PenelopeInferenceLifecycle<S> {
    store: S,
}

impl<S: GraphStore + DurableRecordStore> PenelopeInferenceLifecycle<S> {
    #[must_use]
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Journal a scoped lifecycle request before downstream invalidation.
    ///
    /// # Errors
    /// Rejects invalid controls, changed immutable requests or failed/corrupt effects.
    pub fn invalidate(
        &mut self,
        database: impl AsRef<Path>,
        request: &LifecycleRequest<'_>,
    ) -> Result<LifecycleReceipt, WorkflowError> {
        if request.limits.generations == 0
            || request.limits.versions == 0
            || request.limits.postings == 0
            || request.limits.claims == 0
            || request.evidence_budget == 0
            || request.claim_bytes == 0
        {
            return Err(failure("positive lifecycle execution bounds required"));
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
            .map_err(|_path| failure("lifecycle database path must be UTF-8"))?;
        self.run(&Request {
            origin: request.origin,
            through: request.through,
            database: database_path,
            limits: *request.limits,
            evidence_budget: request.evidence_budget,
            claim_bytes: request.claim_bytes,
        })
    }

    /// Replay bounded record pages and recover prepared effects, including old targets.
    ///
    /// # Errors
    /// Rejects corrupt/substituted records or invalid continuations and failed effects.
    pub fn recover_pending(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = 0_usize;
        loop {
            let page = self
                .store
                .records_with_prefix_page(PREFIX, cursor.as_deref(), 32)?;
            if page.records.len() > 32 {
                return Err(failure("oversized lifecycle recovery page"));
            }
            let mut previous = cursor.clone();
            for (key, bytes) in &page.records {
                if !key.starts_with(PREFIX) || previous.as_ref().is_some_and(|last| key <= last) {
                    return Err(failure("unordered lifecycle recovery key"));
                }
                let record = journal::decode::<Request>(bytes)?;
                if journal::key(&record.request)? != *key {
                    return Err(failure("lifecycle journal key substitution"));
                }
                journal::validate(&record, &record.request)?;
                if record.receipt.is_none() {
                    self.run(&record.request)?;
                    recovered = recovered
                        .checked_add(1)
                        .ok_or_else(|| failure("lifecycle recovery count overflow"))?;
                }
                previous = Some(key.clone());
            }
            let Some(next) = page.next_cursor else {
                return Ok(recovered);
            };
            if previous.as_deref() != Some(next.as_str())
                || cursor.as_ref().is_some_and(|last| &next <= last)
            {
                return Err(failure("nonadvancing lifecycle recovery cursor"));
            }
            cursor = Some(next);
        }
    }

    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }

    fn run(&mut self, request: &Request) -> Result<LifecycleReceipt, WorkflowError> {
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
                            failure("lifecycle journal disappeared during prepare")
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
        let receipt = InferenceMaterializer::open(&request.database)
            .map_err(failure)?
            .invalidate(
                &self.store,
                &LifecycleRequest {
                    origin: request.origin,
                    through: request.through,
                    limits: &request.limits,
                    evidence_budget: request.evidence_budget,
                    claim_bytes: request.claim_bytes,
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
                    .ok_or_else(|| failure("lifecycle journal disappeared during completion"))?;
                let current = journal::decode(&current)?;
                journal::validate(&current, request)?;
                if current.receipt != Some(receipt) {
                    return Err(failure("racing lifecycle completion mismatch"));
                }
                validate_effect(request, receipt)?;
                Ok(receipt)
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn validate_effect(request: &Request, receipt: LifecycleReceipt) -> Result<(), WorkflowError> {
    <Request as journal::Plan>::validate_receipt(request, receipt)?;
    if InferenceMaterializer::open(&request.database)
        .map_err(failure)?
        .lifecycle_receipt(request.origin, request.through)
        .map_err(failure)?
        != Some(receipt)
    {
        return Err(failure(
            "completed lifecycle journal has no matching retained effect",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
