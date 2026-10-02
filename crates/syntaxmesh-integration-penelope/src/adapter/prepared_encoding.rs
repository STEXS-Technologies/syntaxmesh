use super::*;

#[cfg(test)]
mod tests;

#[derive(Serialize)]
struct V1<'record> {
    schema_version: u16,
    run_id: IndexRunId,
    delta: &'record GraphDelta,
    events: &'record [LinearSagaEventEnvelope],
    phase: RecordPhase,
    accepted_generation: Option<&'record GenerationManifest>,
}

#[derive(Serialize)]
struct V2<'record> {
    schema_version: u16,
    run_id: IndexRunId,
    delta: &'record GraphDelta,
    accepted_at: Option<AcceptanceTime>,
    events: &'record [LinearSagaEventEnvelope],
    phase: RecordPhase,
    accepted_generation: Option<&'record GenerationManifest>,
}

#[derive(Serialize)]
struct V3<'record> {
    schema_version: u16,
    run_id: IndexRunId,
    delta: &'record GraphDelta,
    lineage: &'record ChangeSetDelta,
    accepted_at: Option<AcceptanceTime>,
    events: &'record [LinearSagaEventEnvelope],
    phase: RecordPhase,
    accepted_generation: Option<&'record GenerationManifest>,
}

#[derive(Serialize)]
struct V4<'record> {
    schema_version: u16,
    run_id: IndexRunId,
    delta: &'record GraphDelta,
    lineage: &'record ChangeSetDelta,
    consequences: &'record ConsequenceDelta,
    accepted_at: Option<AcceptanceTime>,
    events: &'record [LinearSagaEventEnvelope],
    phase: RecordPhase,
    accepted_generation: Option<&'record GenerationManifest>,
}

pub(super) fn encode(record: &PersistedIndexRun) -> Result<Vec<u8>, WorkflowError> {
    let bytes = match record.schema_version {
        1 => bincode::serialize(&V1 {
            schema_version: 1,
            run_id: record.run_id,
            delta: &record.delta,
            events: &record.events,
            phase: record.phase,
            accepted_generation: record.accepted_generation.as_ref(),
        }),
        2 => bincode::serialize(&V2 {
            schema_version: 2,
            run_id: record.run_id,
            delta: &record.delta,
            accepted_at: record.accepted_at,
            events: &record.events,
            phase: record.phase,
            accepted_generation: record.accepted_generation.as_ref(),
        }),
        3 => bincode::serialize(&V3 {
            schema_version: 3,
            run_id: record.run_id,
            delta: &record.delta,
            lineage: &record.lineage,
            accepted_at: record.accepted_at,
            events: &record.events,
            phase: record.phase,
            accepted_generation: record.accepted_generation.as_ref(),
        }),
        4 => bincode::serialize(&V4 {
            schema_version: 4,
            run_id: record.run_id,
            delta: &record.delta,
            lineage: &record.lineage,
            consequences: &record.consequences,
            accepted_at: record.accepted_at,
            events: &record.events,
            phase: record.phase,
            accepted_generation: record.accepted_generation.as_ref(),
        }),
        version => {
            return Err(WorkflowError::Verification(format!(
                "unsupported Penelope index-record version {version}"
            )));
        }
    };
    bytes.map_err(|error| WorkflowError::Verification(format!("encode Penelope record: {error}")))
}
