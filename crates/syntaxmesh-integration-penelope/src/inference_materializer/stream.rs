use super::{failure, journal};
use serde::{Deserialize, Serialize};
use std::path::Path;
use syntaxmesh_core::GenerationId;
use syntaxmesh_inference::{InvalidationLimits, Limits, Rule, rule_set_identity};
use syntaxmesh_inference_sqlite::{
    InferenceMaterializer, StreamCheckpoint, StreamOrigin, StreamSyncRequest,
};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

const PREFIX: &str = "syntaxmesh.penelope.inference-stream.v1/";

mod begin;
pub use begin::StreamBeginRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Request {
    origin: StreamOrigin,
    through: GenerationId,
    sequence: u64,
    database: String,
    rules: Vec<Rule>,
    limits: Limits,
    range_limits: InvalidationLimits,
    history_scan_budget: usize,
    rule_budget: usize,
    output_bytes: usize,
    premise_budget: usize,
    evidence_budget: usize,
    matched_claim_bytes: usize,
    page_budget: usize,
}

impl Request {
    fn borrowed(&self) -> StreamSyncRequest<'_> {
        StreamSyncRequest {
            origin: self.origin,
            through: self.through,
            rules: &self.rules,
            limits: &self.limits,
            range_limits: &self.range_limits,
            history_scan_budget: self.history_scan_budget,
            rule_budget: self.rule_budget,
            output_bytes: self.output_bytes,
            premise_budget: self.premise_budget,
            evidence_budget: self.evidence_budget,
            matched_claim_bytes: self.matched_claim_bytes,
            page_budget: self.page_budget,
        }
    }
}

impl journal::Plan for Request {
    type Receipt = StreamCheckpoint;
    const DEFINITION: &'static str = "def_syntaxmesh_inference_stream";
    const STEP: &'static str = "inference_stream";

    fn key(&self) -> Result<String, WorkflowError> {
        if rule_set_identity(
            &self.rules,
            self.rule_budget,
            self.limits.input_bytes.min(journal::RULE_BYTES),
        )
        .map_err(failure)?
            != self.origin.baseline.configuration
        {
            return Err(failure("stream journal configuration substitution"));
        }
        let encoded =
            bincode::serialize(&(self.origin, self.through, self.sequence, &self.database))
                .map_err(failure)?;
        Ok(format!(
            "{PREFIX}{:020}/{}",
            self.sequence,
            hex::encode(penelope::ContentDigest::sha256(&encoded).0)
        ))
    }

    fn validate_receipt(&self, receipt: StreamCheckpoint) -> Result<(), WorkflowError> {
        let selection = self.origin.baseline.selection;
        if receipt.stream != self.origin.stream
            || receipt.selection.generation != self.through
            || receipt.selection.repository != selection.repository
            || receipt.selection.worktree != selection.worktree
            || receipt.sequence != self.sequence
            || receipt.added_claims > self.limits.claims
            || receipt.affected_claims > self.range_limits.claims
            || receipt.closure_reasons > self.evidence_budget
        {
            return Err(failure(
                "stream journal receipt scope or admission mismatch",
            ));
        }
        Ok(())
    }
}

/// Recoverable downstream stream initialization and synchronization.
pub struct PenelopeInferenceStream<S> {
    pub(super) store: S,
}

impl<S: GraphStore + DurableRecordStore> PenelopeInferenceStream<S> {
    #[must_use]
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Journal a bounded successor before its atomic downstream effect.
    ///
    /// # Errors
    /// Rejects invalid controls, immutable request changes or failed effects.
    pub fn sync(
        &mut self,
        database: impl AsRef<Path>,
        request: &StreamSyncRequest<'_>,
    ) -> Result<StreamCheckpoint, WorkflowError> {
        if request.page_budget == 0
            || request.output_bytes == 0
            || request.premise_budget == 0
            || request.evidence_budget == 0
            || request.matched_claim_bytes == 0
            || request.history_scan_budget == 0
            || request.range_limits.generations == 0
            || request.range_limits.versions == 0
            || request.range_limits.postings == 0
            || request.range_limits.claims == 0
            || request.limits.facts == 0
            || request.limits.edge_visits == 0
            || request.limits.depth == 0
            || request.limits.claims == 0
        {
            return Err(failure("positive stream execution bounds required"));
        }
        rule_set_identity(
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
            .map_err(|_path| failure("stream database path must be UTF-8"))?;
        self.run(&Request {
            origin: request.origin,
            through: request.through,
            sequence: self.store.generation_sequence(request.through)?,
            database: database_path,
            rules: request.rules.to_vec(),
            limits: *request.limits,
            range_limits: *request.range_limits,
            history_scan_budget: request.history_scan_budget,
            rule_budget: request.rule_budget,
            output_bytes: request.output_bytes,
            premise_budget: request.premise_budget,
            evidence_budget: request.evidence_budget,
            matched_claim_bytes: request.matched_claim_bytes,
            page_budget: request.page_budget,
        })
    }

    /// Recover prepared effects in canonical sequence order from bounded pages.
    ///
    /// # Errors
    /// Rejects corrupt records, invalid continuations and failed effects.
    pub fn recover_pending(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = self.recover_begins()?;
        loop {
            let page = self
                .store
                .records_with_prefix_page(PREFIX, cursor.as_deref(), 32)?;
            if page.records.len() > 32 {
                return Err(failure("oversized stream recovery page"));
            }
            let mut previous = cursor.clone();
            for (key, bytes) in &page.records {
                if !key.starts_with(PREFIX) || previous.as_ref().is_some_and(|last| key <= last) {
                    return Err(failure("unordered stream recovery key"));
                }
                let record = journal::decode::<Request>(bytes)?;
                if journal::key(&record.request)? != *key {
                    return Err(failure("stream journal key substitution"));
                }
                journal::validate(&record, &record.request)?;
                if record.receipt.is_none() {
                    self.run(&record.request)?;
                    recovered = recovered
                        .checked_add(1)
                        .ok_or_else(|| failure("stream recovery count overflow"))?;
                }
                previous = Some(key.clone());
            }
            let Some(next) = page.next_cursor else {
                recovered = recovered
                    .checked_add(self.recover_registered_stream_pending()?)
                    .ok_or_else(|| failure("registered stream recovery count overflow"))?;
                return Ok(recovered);
            };
            if previous.as_deref() != Some(next.as_str())
                || cursor.as_ref().is_some_and(|last| &next <= last)
            {
                return Err(failure("nonadvancing stream recovery cursor"));
            }
            cursor = Some(next);
        }
    }

    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }

    fn run(&mut self, request: &Request) -> Result<StreamCheckpoint, WorkflowError> {
        if self.store.generation_sequence(request.through)? != request.sequence {
            return Err(failure("stream journal canonical sequence mismatch"));
        }
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
                        let raced = self
                            .store
                            .read_record(&key)?
                            .ok_or_else(|| failure("stream journal disappeared during prepare"))?;
                        let raced_record = journal::decode(&raced)?;
                        journal::validate(&raced_record, request)?;
                        (raced_record, raced)
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };
        if let Some(receipt) = record.receipt {
            self.validate_effect(request, receipt)?;
            return Ok(receipt);
        }
        let receipt = InferenceMaterializer::open(&request.database)
            .map_err(failure)?
            .sync_stream(&self.store, &request.borrowed())
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
                    .ok_or_else(|| failure("stream journal disappeared during completion"))?;
                let current = journal::decode(&current)?;
                journal::validate(&current, request)?;
                if current.receipt != Some(receipt) {
                    return Err(failure("racing stream completion mismatch"));
                }
                self.validate_effect(request, receipt)?;
                Ok(receipt)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn validate_effect(
        &self,
        request: &Request,
        receipt: StreamCheckpoint,
    ) -> Result<(), WorkflowError> {
        <Request as journal::Plan>::validate_receipt(request, receipt)?;
        let mut materializer = InferenceMaterializer::open(&request.database).map_err(failure)?;
        if materializer
            .stream_checkpoint(request.origin, request.through)
            .map_err(failure)?
            != Some(receipt)
            || materializer
                .sync_stream(&self.store, &request.borrowed())
                .map_err(failure)?
                != receipt
        {
            return Err(failure(
                "completed stream journal has no matching retained effect",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
