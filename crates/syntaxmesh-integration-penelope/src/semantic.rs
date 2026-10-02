//! Restartable semantic enrichment and content-addressed result reuse.

use penelope::{
    ActionId, ActionResultObservation, ContentDigest, DefinitionId, DefinitionVersion, InputId,
    LinearSagaDefinition, LinearSagaEventEnvelope, LinearSagaInput, ProcessActionKind, ProcessId,
    RetryPolicy, SagaDecision, SagaStatus, StepId, StepPlan, TenantId, apply_action_result, replay,
    start,
};
use serde::{Deserialize, Serialize};
use syntaxmesh_core::StableId;
use syntaxmesh_semantic::{SemanticOutput, SemanticProvider, SemanticRequest};
use syntaxmesh_store::{DurableRecordStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

const RECORD_PREFIX: &str = "syntaxmesh.penelope.semantic.v1/";
const RECOVERY_PAGE_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum RecordPhase {
    Prepared,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SemanticProcessRecord {
    schema_version: u16,
    cache_key: StableId,
    request_digest: ContentDigest,
    definition_digest: ContentDigest,
    request: Option<SemanticRequest>,
    output: Option<SemanticOutput>,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
}

struct SagaContext {
    definition: LinearSagaDefinition,
    tenant_id: TenantId,
    process_id: ProcessId,
    action_id: ActionId,
}

/// Penelope-backed semantic job journal and content-addressed output cache.
///
/// The graph store's durable-record CAS port is the only persistence used.
/// Callers explicitly publish the resulting `FactBatch` through the ordinary
/// engine extension workflow.
pub struct PenelopeSemanticEnricher<'store, S> {
    store: &'store mut S,
}

struct PendingSemanticEnrichment {
    position: usize,
    request: SemanticRequest,
    record: SemanticProcessRecord,
    encoded: Vec<u8>,
    context: SagaContext,
    key: String,
}

impl<'store, S> PenelopeSemanticEnricher<'store, S>
where
    S: DurableRecordStore,
{
    /// Create an enricher over a graph store's durable-record capability.
    #[must_use]
    pub const fn new(store: &'store mut S) -> Self {
        Self { store }
    }

    /// Reuse or compute source-grounded semantic output and bind it to `request`.
    ///
    /// A provider/evidence failure leaves the Penelope operation prepared for
    /// retry. Completed output is keyed only by content and semantic identity,
    /// so it can be safely rebound to another generation's physical nodes.
    ///
    /// # Errors
    /// Returns an error on corrupt/conflicting workflow records, provider
    /// failure, invalid evidence, or a durable-record store failure.
    pub fn enrich<P: SemanticProvider>(
        &mut self,
        request: SemanticRequest,
        provider: &P,
    ) -> Result<syntaxmesh_extension_sdk::FactBatch, WorkflowError> {
        self.enrich_many(vec![request], provider)
    }

    /// Prepare every request durably, run missing provider work as one
    /// provider batch, then complete each validated result through CAS. An
    /// error leaves unsuccessful records prepared; successful records stay
    /// cached for an inexpensive retry of the full operation.
    ///
    /// # Errors
    /// Returns an error on corrupt/conflicting workflow records, provider
    /// failure, invalid evidence, or a durable-record store failure.
    pub fn enrich_many<P: SemanticProvider>(
        &mut self,
        requests: Vec<SemanticRequest>,
        provider: &P,
    ) -> Result<syntaxmesh_extension_sdk::FactBatch, WorkflowError> {
        let (request, output) = self.enrich_outputs_many(requests, provider)?;
        semantic_fact_batch(request, output)
    }

    /// Return validated, durably cached output for explicit layer composition.
    /// No graph facts are published by this operation.
    ///
    /// # Errors
    /// Returns the same preparation, provider, evidence, and cache errors as
    /// [`Self::enrich_many`]. Successful requests remain cached on failure.
    pub fn enrich_outputs_many<P: SemanticProvider>(
        &mut self,
        requests: Vec<SemanticRequest>,
        provider: &P,
    ) -> Result<(SemanticRequest, SemanticOutput), WorkflowError> {
        if requests.is_empty() {
            return Err(WorkflowError::Verification(
                "semantic enrichment requires at least one request".to_owned(),
            ));
        }
        let combined_request = SemanticRequest::combine(requests.clone())
            .map_err(|error| WorkflowError::Verification(format!("semantic request: {error}")))?;
        let provider_identity = provider.identity();
        let mut results = vec![None; requests.len()];
        let mut pending = Vec::new();
        for (position, request) in requests.into_iter().enumerate() {
            request.validate().map_err(|error| {
                WorkflowError::Verification(format!("semantic request: {error}"))
            })?;
            if request.identity() != &provider_identity {
                return Err(WorkflowError::Verification(
                    "semantic provider identity does not match the request".to_owned(),
                ));
            }
            let cache_key = request.cache_key();
            let key = record_key(cache_key);
            let (operation_digest, request_digest) = request_digests(cache_key)?;
            let context = saga_context(operation_digest, request_digest)?;
            let (record, encoded) = load_or_prepare(
                self.store,
                &request,
                &key,
                cache_key,
                request_digest,
                &context,
            )?;
            match record.phase {
                RecordPhase::Completed => {
                    let output = record.output.ok_or_else(|| {
                        WorkflowError::Verification(
                            "completed semantic process has no structured output".to_owned(),
                        )
                    })?;
                    let slot = results.get_mut(position).ok_or_else(|| {
                        WorkflowError::Verification(
                            "semantic request position is out of range".to_owned(),
                        )
                    })?;
                    *slot = Some(output);
                }
                RecordPhase::Prepared => pending.push(PendingSemanticEnrichment {
                    position,
                    request,
                    record,
                    encoded,
                    context,
                    key,
                }),
            }
        }

        let provider_requests = pending
            .iter()
            .map(|work| work.request.clone())
            .collect::<Vec<_>>();
        let provider_outputs = provider.extract_many(&provider_requests);
        if provider_outputs.len() != pending.len() {
            return Err(WorkflowError::Verification(
                "semantic provider returned a different number of batch results".to_owned(),
            ));
        }
        let mut provider_failure = None;
        for (work, output) in pending.into_iter().zip(provider_outputs) {
            let output = match output {
                Ok(output) => output,
                Err(error) => {
                    provider_failure.get_or_insert(error);
                    continue;
                }
            };
            // Validate before completing the durable job so invalid model
            // output remains retryable and can never enter the graph.
            work.request
                .clone()
                .into_fact_batch(output.clone())
                .map_err(|error| {
                    WorkflowError::Verification(format!("semantic evidence: {error}"))
                })?;
            let completed = complete_record(work.record, &work.context, output.clone())?;
            let replacement = encode_record(&completed)?;
            let cache_key = work.request.cache_key();
            let (_, request_digest) = request_digests(cache_key)?;
            let output = match self.store.compare_exchange_record(
                &work.key,
                Some(&work.encoded),
                &replacement,
            ) {
                Ok(()) => output,
                Err(StoreError::RecordConflict(_)) => {
                    let current = self.store.read_record(&work.key)?.ok_or_else(|| {
                        WorkflowError::Verification(
                            "semantic process disappeared during completion".to_owned(),
                        )
                    })?;
                    let current = decode_record(&current)?;
                    validate_record(&current, cache_key, request_digest, &work.context)?;
                    if current.phase != RecordPhase::Completed {
                        return Err(WorkflowError::Store(StoreError::RecordConflict(work.key)));
                    }
                    current.output.ok_or_else(|| {
                        WorkflowError::Verification(
                            "racing completed semantic process has no output".to_owned(),
                        )
                    })?
                }
                Err(error) => return Err(WorkflowError::Store(error)),
            };
            let slot = results.get_mut(work.position).ok_or_else(|| {
                WorkflowError::Verification("semantic request position is out of range".to_owned())
            })?;
            *slot = Some(output);
        }
        if let Some(error) = provider_failure {
            return Err(WorkflowError::Verification(format!(
                "semantic provider: {error}"
            )));
        }
        let merged_outputs = results
            .into_iter()
            .map(|output| {
                output.ok_or_else(|| {
                    WorkflowError::Verification(
                        "semantic enrichment omitted a prepared result".to_owned(),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let output = SemanticOutput::merge(merged_outputs);
        Ok((combined_request, output))
    }

    /// Resume all prepared semantic jobs with bounded journal pages.
    ///
    /// # Errors
    /// Returns an error if a persisted request is corrupt, its provider
    /// identity no longer matches, or a provider/store operation fails.
    pub fn recover_pending<P: SemanticProvider>(
        &mut self,
        provider: &P,
    ) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = 0_usize;
        loop {
            let page = self.store.records_with_prefix_page(
                RECORD_PREFIX,
                cursor.as_deref(),
                RECOVERY_PAGE_SIZE,
            )?;
            for (key, bytes) in page.records {
                let record = decode_record(&bytes)?;
                let (operation_digest, request_digest) = request_digests(record.cache_key)?;
                let context = saga_context(operation_digest, request_digest)?;
                validate_record(&record, record.cache_key, request_digest, &context)?;
                if record_key(record.cache_key) != key {
                    return Err(WorkflowError::Verification(
                        "semantic workflow key does not match its cache identity".to_owned(),
                    ));
                }
                if record.phase == RecordPhase::Prepared {
                    let request = record.request.ok_or_else(|| {
                        WorkflowError::Verification(
                            "prepared semantic process has no resumable request".to_owned(),
                        )
                    })?;
                    if request.identity() == &provider.identity() {
                        self.enrich(request, provider)?;
                        recovered = recovered.saturating_add(1);
                    }
                }
            }
            let Some(next_cursor) = page.next_cursor else {
                return Ok(recovered);
            };
            cursor = Some(next_cursor);
        }
    }
}

fn semantic_fact_batch(
    request: SemanticRequest,
    output: SemanticOutput,
) -> Result<syntaxmesh_extension_sdk::FactBatch, WorkflowError> {
    request
        .into_fact_batch(output)
        .map_err(|error| WorkflowError::Verification(format!("semantic evidence: {error}")))
}

fn load_or_prepare<S: DurableRecordStore>(
    store: &mut S,
    request: &SemanticRequest,
    key: &str,
    cache_key: StableId,
    request_digest: ContentDigest,
    context: &SagaContext,
) -> Result<(SemanticProcessRecord, Vec<u8>), WorkflowError> {
    match store.read_record(key)? {
        Some(encoded) => {
            let record = decode_record(&encoded)?;
            validate_record(&record, cache_key, request_digest, context)?;
            Ok((record, encoded))
        }
        None => {
            let record = prepare_record(context, cache_key, request_digest, request.clone())?;
            let bytes = encode_record(&record)?;
            match store.compare_exchange_record(key, None, &bytes) {
                Ok(()) => Ok((record, bytes)),
                Err(StoreError::RecordConflict(_)) => {
                    let raced = store.read_record(key)?.ok_or_else(|| {
                        WorkflowError::Verification(
                            "semantic process raced creation but disappeared".to_owned(),
                        )
                    })?;
                    let raced_record = decode_record(&raced)?;
                    validate_record(&raced_record, cache_key, request_digest, context)?;
                    Ok((raced_record, raced))
                }
                Err(error) => Err(WorkflowError::Store(error)),
            }
        }
    }
}

fn request_digests(cache_key: StableId) -> Result<(ContentDigest, ContentDigest), WorkflowError> {
    let bytes = bincode::serialize(&cache_key).map_err(|error| {
        WorkflowError::Verification(format!("encode semantic cache identity: {error}"))
    })?;
    let digest = ContentDigest::sha256(&bytes);
    Ok((digest, digest))
}

fn saga_context(
    operation_digest: ContentDigest,
    request_digest: ContentDigest,
) -> Result<SagaContext, WorkflowError> {
    let suffix = hex::encode(operation_digest.0);
    let definition = LinearSagaDefinition::new(
        DefinitionId::new(String::from("def_syntaxmesh_semantic_enrichment"))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        DefinitionVersion::new(String::from("dfv_v1"))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        request_digest,
        vec![StepPlan {
            step_id: StepId::new(format!("stp_semantic_{suffix}"))
                .map_err(|error| WorkflowError::Verification(error.to_string()))?,
            action_kind: ProcessActionKind::ExternalEffect,
            payload_digest: request_digest,
            retry_policy: RetryPolicy::no_retry(),
            compensation: None,
        }],
    );
    Ok(SagaContext {
        definition,
        tenant_id: TenantId::new(String::from("tnt_syntaxmesh"))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        process_id: ProcessId::new(format!("prc_{suffix}"))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        action_id: ActionId::new(format!("act_{suffix}"))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
    })
}

fn prepare_record(
    context: &SagaContext,
    cache_key: StableId,
    request_digest: ContentDigest,
    request: SemanticRequest,
) -> Result<SemanticProcessRecord, WorkflowError> {
    let decision = start(
        &context.definition,
        context.tenant_id.clone(),
        context.process_id.clone(),
        context.action_id.clone(),
    )
    .map_err(|error| WorkflowError::Verification(format!("start semantic process: {error}")))?;
    let event = LinearSagaInput::Start {
        input_id: InputId::new(format!("inp_start_{}", context.process_id.as_str()))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        action_id: context.action_id.clone(),
    }
    .to_event(&context.definition);
    let record = SemanticProcessRecord {
        schema_version: 1,
        cache_key,
        request_digest,
        definition_digest: request_digest,
        request: Some(request),
        output: None,
        events: vec![LinearSagaEventEnvelope { sequence: 0, event }],
        phase: RecordPhase::Prepared,
    };
    if replay_record(&record, context)? != decision {
        return Err(WorkflowError::Verification(
            "semantic process start event failed replay validation".to_owned(),
        ));
    }
    Ok(record)
}

fn complete_record(
    mut record: SemanticProcessRecord,
    context: &SagaContext,
    output: SemanticOutput,
) -> Result<SemanticProcessRecord, WorkflowError> {
    let started = replay_record(&record, context)?;
    let action_id = started.projection.active_action_id.clone().ok_or_else(|| {
        WorkflowError::Verification("semantic process has no active extraction action".to_owned())
    })?;
    let observation = ActionResultObservation::succeeded(action_id);
    let decided = apply_action_result(
        &context.definition,
        &started.projection,
        context.tenant_id.clone(),
        context.process_id.clone(),
        &observation,
        None,
    )
    .map_err(|error| WorkflowError::Verification(format!("complete semantic process: {error}")))?;
    let event = LinearSagaInput::ActionResult {
        input_id: InputId::new(format!("inp_complete_{}", context.process_id.as_str()))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        observation,
        next_action_id: None,
    }
    .to_event(&context.definition);
    record.events.push(LinearSagaEventEnvelope {
        sequence: u64::try_from(record.events.len())
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        event,
    });
    if replay_record(&record, context)? != decided
        || decided.projection.status != SagaStatus::Completed
    {
        return Err(WorkflowError::Verification(
            "completed semantic process failed replay validation".to_owned(),
        ));
    }
    record.phase = RecordPhase::Completed;
    record.request = None;
    record.output = Some(output);
    Ok(record)
}

fn replay_record(
    record: &SemanticProcessRecord,
    context: &SagaContext,
) -> Result<SagaDecision, WorkflowError> {
    if record.schema_version != 1 || record.definition_digest != record.request_digest {
        return Err(WorkflowError::Verification(
            "unsupported or inconsistent semantic workflow record".to_owned(),
        ));
    }
    let mut events = Vec::with_capacity(record.events.len());
    for (expected, envelope) in record.events.iter().enumerate() {
        if usize::try_from(envelope.sequence).ok() != Some(expected) {
            return Err(WorkflowError::Verification(
                "semantic workflow event sequence is not contiguous".to_owned(),
            ));
        }
        events.push(envelope.event.clone());
    }
    replay(
        &context.definition,
        &context.tenant_id,
        &context.process_id,
        &events,
    )
    .map_err(|error| WorkflowError::Verification(format!("replay semantic process: {error}")))
}

fn validate_record(
    record: &SemanticProcessRecord,
    cache_key: StableId,
    request_digest: ContentDigest,
    context: &SagaContext,
) -> Result<(), WorkflowError> {
    if record.schema_version != 1
        || record.cache_key != cache_key
        || record.request_digest != request_digest
        || record.definition_digest != request_digest
    {
        return Err(WorkflowError::Verification(
            "semantic cache identity was reused with inconsistent content".to_owned(),
        ));
    }
    if let Some(request) = &record.request
        && request.cache_key() != cache_key
    {
        return Err(WorkflowError::Verification(
            "prepared semantic request disagrees with its cache key".to_owned(),
        ));
    }
    if let Some(request) = &record.request {
        request.validate().map_err(|error| {
            WorkflowError::Verification(format!("persisted semantic request: {error}"))
        })?;
    }
    let decision = replay_record(record, context)?;
    let phase_is_valid = match record.phase {
        RecordPhase::Prepared => {
            decision.projection.status == SagaStatus::Running
                && record.request.is_some()
                && record.output.is_none()
        }
        RecordPhase::Completed => {
            decision.projection.status == SagaStatus::Completed
                && record.request.is_none()
                && record.output.is_some()
        }
    };
    if !phase_is_valid {
        return Err(WorkflowError::Verification(
            "semantic workflow phase disagrees with replayed Penelope state".to_owned(),
        ));
    }
    Ok(())
}

fn encode_record(record: &SemanticProcessRecord) -> Result<Vec<u8>, WorkflowError> {
    bincode::serialize(record)
        .map_err(|error| WorkflowError::Verification(format!("encode semantic record: {error}")))
}

fn decode_record(bytes: &[u8]) -> Result<SemanticProcessRecord, WorkflowError> {
    bincode::deserialize(bytes)
        .map_err(|error| WorkflowError::Verification(format!("decode semantic record: {error}")))
}

fn record_key(cache_key: StableId) -> String {
    format!("{RECORD_PREFIX}{}", cache_key.to_hex())
}

#[cfg(test)]
mod tests;
