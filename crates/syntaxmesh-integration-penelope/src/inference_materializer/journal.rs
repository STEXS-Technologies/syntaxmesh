use super::{Request, failure};
use bincode::Options;
use penelope::{
    ActionId, ActionResultObservation, ContentDigest, DefinitionId, DefinitionVersion, InputId,
    LinearSagaDefinition, LinearSagaEventEnvelope, LinearSagaInput, ProcessActionKind, ProcessId,
    RetryPolicy, SagaDecision, SagaStatus, StepId, StepPlan, TenantId, apply_action_result, replay,
    start,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use syntaxmesh_inference_sqlite::Watermark;
use syntaxmesh_workflow::WorkflowError;

const RECORD_BYTES: u64 = 16_777_216;
pub(super) const RULE_BYTES: usize = 8_388_608;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Record<P = Request, R = Watermark> {
    version: u16,
    pub(super) request: P,
    digest: ContentDigest,
    events: Vec<LinearSagaEventEnvelope>,
    pub(super) receipt: Option<R>,
}

pub(super) trait Plan: Clone + PartialEq + Serialize + DeserializeOwned {
    type Receipt: Copy + PartialEq + Serialize + DeserializeOwned;
    const DEFINITION: &'static str;
    const STEP: &'static str;
    fn key(&self) -> Result<String, WorkflowError>;
    fn validate_receipt(&self, receipt: Self::Receipt) -> Result<(), WorkflowError>;
}

struct Context {
    definition: LinearSagaDefinition,
    tenant: TenantId,
    process: ProcessId,
    action: ActionId,
}

pub(super) fn key<P: Plan>(request: &P) -> Result<String, WorkflowError> {
    request.key()
}

fn digest<P: Plan>(request: &P) -> Result<ContentDigest, WorkflowError> {
    if bincode::serialized_size(request).map_err(failure)? > RECORD_BYTES {
        return Err(failure("inference request exceeds journal byte bound"));
    }
    Ok(ContentDigest::sha256(
        &bincode::serialize(request).map_err(failure)?,
    ))
}

fn context<P: Plan>(request: &P) -> Result<Context, WorkflowError> {
    let suffix = key(request)?;
    let suffix = suffix
        .rsplit('/')
        .next()
        .ok_or_else(|| failure("inference key prefix mismatch"))?;
    let digest = digest(request)?;
    Ok(Context {
        definition: LinearSagaDefinition::new(
            DefinitionId::new(P::DEFINITION.to_owned()).map_err(failure)?,
            DefinitionVersion::new("dfv_v1".to_owned()).map_err(failure)?,
            digest,
            vec![StepPlan {
                step_id: StepId::new(format!("stp_{}_{suffix}", P::STEP)).map_err(failure)?,
                action_kind: ProcessActionKind::ExternalEffect,
                payload_digest: digest,
                retry_policy: RetryPolicy::no_retry(),
                compensation: None,
            }],
        ),
        tenant: TenantId::new("tnt_syntaxmesh".to_owned()).map_err(failure)?,
        process: ProcessId::new(format!("prc_{suffix}")).map_err(failure)?,
        action: ActionId::new(format!("act_{suffix}")).map_err(failure)?,
    })
}

pub(super) fn prepare<P: Plan>(request: P) -> Result<Record<P, P::Receipt>, WorkflowError> {
    let context = context(&request)?;
    let started = start(
        &context.definition,
        context.tenant.clone(),
        context.process.clone(),
        context.action.clone(),
    )
    .map_err(failure)?;
    let event = LinearSagaInput::Start {
        input_id: InputId::new(format!("inp_start_{}", context.process.as_str()))
            .map_err(failure)?,
        action_id: context.action,
    }
    .to_event(&context.definition);
    let record = Record {
        version: 1,
        digest: digest(&request)?,
        request,
        events: vec![LinearSagaEventEnvelope { sequence: 0, event }],
        receipt: None,
    };
    if replay_record(&record)? != started {
        return Err(failure("inference start replay mismatch"));
    }
    Ok(record)
}

pub(super) fn complete<P: Plan>(
    mut record: Record<P, P::Receipt>,
    receipt: P::Receipt,
) -> Result<Record<P, P::Receipt>, WorkflowError> {
    validate(&record, &record.request)?;
    let context = context(&record.request)?;
    let started = replay_record(&record)?;
    let active = started
        .projection
        .active_action_id
        .clone()
        .ok_or_else(|| failure("inference process has no active action"))?;
    let observation = ActionResultObservation::succeeded(active);
    let completed = apply_action_result(
        &context.definition,
        &started.projection,
        context.tenant.clone(),
        context.process.clone(),
        &observation,
        None,
    )
    .map_err(failure)?;
    let event = LinearSagaInput::ActionResult {
        input_id: InputId::new(format!("inp_complete_{}", context.process.as_str()))
            .map_err(failure)?,
        observation,
        next_action_id: None,
    }
    .to_event(&context.definition);
    record
        .events
        .push(LinearSagaEventEnvelope { sequence: 1, event });
    record.receipt = Some(receipt);
    if replay_record(&record)? != completed {
        return Err(failure("inference completion replay mismatch"));
    }
    validate(&record, &record.request)?;
    Ok(record)
}

fn replay_record<P: Plan>(record: &Record<P, P::Receipt>) -> Result<SagaDecision, WorkflowError> {
    if record.version != 1 || record.events.len() != if record.receipt.is_some() { 2 } else { 1 } {
        return Err(failure("inference journal version or event count mismatch"));
    }
    let context = context(&record.request)?;
    let mut events = Vec::with_capacity(record.events.len());
    for (expected, envelope) in record.events.iter().enumerate() {
        if usize::try_from(envelope.sequence).ok() != Some(expected) {
            return Err(failure("noncontiguous inference journal events"));
        }
        events.push(envelope.event.clone());
    }
    replay(
        &context.definition,
        &context.tenant,
        &context.process,
        &events,
    )
    .map_err(failure)
}

pub(super) fn validate<P: Plan>(
    record: &Record<P, P::Receipt>,
    request: &P,
) -> Result<(), WorkflowError> {
    if &record.request != request || record.digest != digest(request)? {
        return Err(failure("inference request content/digest changed"));
    }
    let replayed = replay_record(record)?;
    let expected = if let Some(receipt) = record.receipt {
        request.validate_receipt(receipt)?;
        SagaStatus::Completed
    } else {
        SagaStatus::Running
    };
    if replayed.projection.status != expected {
        return Err(failure("inference receipt disagrees with saga state"));
    }
    Ok(())
}

pub(super) fn encode<P: Plan>(record: &Record<P, P::Receipt>) -> Result<Vec<u8>, WorkflowError> {
    if bincode::serialized_size(record).map_err(failure)? > RECORD_BYTES {
        return Err(failure("inference journal byte bound exhausted"));
    }
    bincode::serialize(record).map_err(failure)
}

pub(super) fn decode<P: Plan>(bytes: &[u8]) -> Result<Record<P, P::Receipt>, WorkflowError> {
    if u64::try_from(bytes.len()).map_err(failure)? > RECORD_BYTES {
        return Err(failure("oversized inference journal record"));
    }
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(RECORD_BYTES)
        .reject_trailing_bytes()
        .deserialize(bytes)
        .map_err(failure)
}
