use super::{
    PenelopeInferenceMaterializer, PenelopeInferenceStream, StreamBeginRequest, failure, journal,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use syntaxmesh_core::StableId;
use syntaxmesh_inference::{ClaimSelection, materialization_identity};
use syntaxmesh_inference_sqlite::{InferenceMaterializer, RebuildRequest, StreamOrigin};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

const PREFIX: &str = "syntaxmesh.penelope.inference-setup.v1/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Request {
    rebuild: super::Request,
    page_budget: usize,
}

impl journal::Plan for Request {
    type Receipt = StreamOrigin;
    const DEFINITION: &'static str = "def_syntaxmesh_inference_setup";
    const STEP: &'static str = "inference_setup";

    fn key(&self) -> Result<String, WorkflowError> {
        let child_key = journal::key(&self.rebuild)?;
        let suffix = child_key
            .rsplit('/')
            .next()
            .ok_or_else(|| failure("missing setup identity"))?;
        Ok(format!("{PREFIX}{suffix}"))
    }

    fn validate_receipt(&self, receipt: StreamOrigin) -> Result<(), WorkflowError> {
        if receipt.baseline.operation != self.rebuild.operation
            || receipt.baseline.selection != self.rebuild.selection
            || receipt.baseline.claim_count > self.rebuild.limits.claims
            || receipt.stream
                != StableId::derive("inference-stream-v1", &[&receipt.baseline.operation.0])
        {
            return Err(failure("setup receipt identity or admission mismatch"));
        }
        Ok(())
    }
}

/// Parent setup intent survives interruption before either child is prepared.
pub struct PenelopeInferenceSetup<S> {
    pub(super) store: S,
}

impl<S: GraphStore + DurableRecordStore> PenelopeInferenceSetup<S> {
    #[must_use]
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Journal whole-command intent before rebuilding and initializing a stream.
    ///
    /// # Errors
    /// Rejects invalid bounds, changed requests, malformed journals and failed effects.
    pub fn initialize(
        &mut self,
        database: impl AsRef<Path>,
        request: &RebuildRequest<'_>,
        page_budget: usize,
    ) -> Result<StreamOrigin, WorkflowError> {
        if page_budget == 0
            || request.history_scan_budget == 0
            || request.output_bytes == 0
            || request.premise_budget == 0
            || request.limits.facts == 0
            || request.limits.edge_visits == 0
            || request.limits.depth == 0
            || request.limits.claims == 0
        {
            return Err(failure("positive inference setup admission required"));
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
            .map_err(|_path| failure("setup database path must be UTF-8"))?;
        let mut rules = request.rules.to_vec();
        rules.sort_by(|left, right| (&left.id, &left.version).cmp(&(&right.id, &right.version)));
        self.run(&Request {
            rebuild: super::Request {
                selection,
                operation,
                database: database_path,
                rules,
                limits: *request.limits,
                history_scan_budget: request.history_scan_budget,
                rule_budget: request.rule_budget,
                output_bytes: request.output_bytes,
                premise_budget: request.premise_budget,
            },
            page_budget,
        })
    }

    /// Recover parent setup intents through bounded, ordered durable record pages.
    ///
    /// # Errors
    /// Rejects malformed records/continuations and unavailable child effects.
    pub fn recover_pending(&mut self) -> Result<usize, WorkflowError> {
        let registered = self.recover_registered_pending()?;
        self.recover_unregistered_pending()?
            .checked_add(registered)
            .ok_or_else(|| failure("setup recovery count overflow"))
    }

    fn recover_unregistered_pending(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = 0_usize;
        loop {
            let page = self
                .store
                .records_with_prefix_page(PREFIX, cursor.as_deref(), 32)?;
            if page.records.len() > 32 {
                return Err(failure("oversized setup recovery page"));
            }
            let mut previous = cursor.clone();
            for (key, bytes) in &page.records {
                if !key.starts_with(PREFIX) || previous.as_ref().is_some_and(|last| key <= last) {
                    return Err(failure("unordered setup recovery key"));
                }
                let record = journal::decode::<Request>(bytes)?;
                if journal::key(&record.request)? != *key {
                    return Err(failure("setup key substitution"));
                }
                journal::validate(&record, &record.request)?;
                if record.receipt.is_none() {
                    self.run(&record.request)?;
                    recovered = recovered
                        .checked_add(1)
                        .ok_or_else(|| failure("setup recovery count overflow"))?;
                }
                previous = Some(key.clone());
            }
            let Some(next) = page.next_cursor else {
                return Ok(recovered);
            };
            if previous.as_deref() != Some(next.as_str())
                || cursor.as_ref().is_some_and(|last| &next <= last)
            {
                return Err(failure("nonadvancing setup recovery cursor"));
            }
            cursor = Some(next);
        }
    }

    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }

    fn run(&mut self, request: &Request) -> Result<StreamOrigin, WorkflowError> {
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
                            .ok_or_else(|| failure("setup journal disappeared during prepare"))?;
                        let raced_record = journal::decode(&raced)?;
                        journal::validate(&raced_record, request)?;
                        (raced_record, raced)
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };
        if let Some(receipt) = record.receipt {
            self.validate_completed(request, receipt)?;
            return Ok(receipt);
        }
        let receipt = self.children(request)?;
        let replacement = journal::encode(&journal::complete(record, receipt)?)?;
        match self
            .store
            .compare_exchange_record(&key, Some(&bytes), &replacement)
        {
            Ok(()) => Ok(receipt),
            Err(StoreError::RecordConflict(_)) => {
                let current = self
                    .store
                    .read_record(&key)?
                    .ok_or_else(|| failure("setup journal disappeared during completion"))?;
                let current = journal::decode(&current)?;
                journal::validate(&current, request)?;
                if current.receipt != Some(receipt) {
                    return Err(failure("racing setup completion mismatch"));
                }
                self.validate_completed(request, receipt)?;
                Ok(receipt)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn children(&mut self, request: &Request) -> Result<StreamOrigin, WorkflowError> {
        let plan = &request.rebuild;
        let baseline = PenelopeInferenceMaterializer::new(&mut self.store).rebuild(
            &plan.database,
            &RebuildRequest {
                generation: plan.selection.generation,
                rules: &plan.rules,
                limits: &plan.limits,
                history_scan_budget: plan.history_scan_budget,
                rule_budget: plan.rule_budget,
                output_bytes: plan.output_bytes,
                premise_budget: plan.premise_budget,
            },
        )?;
        PenelopeInferenceStream::new(&mut self.store).begin(
            &plan.database,
            &StreamBeginRequest {
                baseline,
                claim_budget: plan.limits.claims,
                byte_budget: plan.output_bytes,
                premise_budget: plan.premise_budget,
                page_budget: request.page_budget,
            },
        )
    }

    fn validate_completed(
        &mut self,
        request: &Request,
        receipt: StreamOrigin,
    ) -> Result<(), WorkflowError> {
        <Request as journal::Plan>::validate_receipt(request, receipt)?;
        let child_key = journal::key(&request.rebuild)?;
        let bytes = self
            .store
            .read_record(&child_key)?
            .ok_or_else(|| failure("completed setup baseline journal missing"))?;
        let child = journal::decode::<super::Request>(&bytes)?;
        journal::validate(&child, &request.rebuild)?;
        if child.receipt != Some(receipt.baseline) {
            return Err(failure("completed setup baseline receipt mismatch"));
        }
        let materializer =
            InferenceMaterializer::open(&request.rebuild.database).map_err(failure)?;
        let checkpoint = materializer
            .stream_checkpoint(receipt, request.rebuild.selection.generation)
            .map_err(failure)?
            .ok_or_else(|| failure("completed setup stream effect missing"))?;
        if checkpoint.sequence != receipt.baseline.generation_sequence
            || checkpoint.active_claims != receipt.baseline.claim_count
        {
            return Err(failure("completed setup checkpoint mismatch"));
        }
        drop(materializer);
        if self.children(request)? != receipt {
            return Err(failure("completed setup child effect mismatch"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
