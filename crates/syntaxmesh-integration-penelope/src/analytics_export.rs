//! Durable orchestration for downstream analytical Parquet exports.

use std::path::{Path, PathBuf};

use penelope::{
    ActionId, ActionResultObservation, ContentDigest, DefinitionId, DefinitionVersion, InputId,
    LinearSagaDefinition, LinearSagaEventEnvelope, LinearSagaInput, ProcessActionKind, ProcessId,
    RetryPolicy, SagaDecision, SagaStatus, StepId, StepPlan, TenantId, apply_action_result, replay,
    start,
};
use serde::{Deserialize, Serialize};
use syntaxmesh_analytics::MAX_FACT_HISTORY_PAGE_SIZE;
use syntaxmesh_analytics_parquet::{
    FactHistoryParquetExport, ParquetExportError, export_fact_history,
};
use syntaxmesh_core::{GenerationId, RepositoryId, WorktreeId};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

const RECORD_PREFIX: &str = "syntaxmesh.penelope.analytics-export.v1/";
const RECOVERY_PAGE_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum RecordPhase {
    Prepared,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedReceipt {
    partition_count: u64,
    row_count: u64,
    complete: bool,
}

impl From<FactHistoryParquetExport> for PersistedReceipt {
    fn from(export: FactHistoryParquetExport) -> Self {
        Self {
            partition_count: export.partition_count,
            row_count: export.row_count,
            complete: export.complete,
        }
    }
}

impl From<PersistedReceipt> for FactHistoryParquetExport {
    fn from(receipt: PersistedReceipt) -> Self {
        Self {
            partition_count: receipt.partition_count,
            row_count: receipt.row_count,
            complete: receipt.complete,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ExportRequest {
    repository: RepositoryId,
    worktree: WorktreeId,
    generation: GenerationId,
    destination: String,
    page_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ExportProcessRecord {
    schema_version: u16,
    request: ExportRequest,
    request_digest: ContentDigest,
    definition_digest: ContentDigest,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
    receipt: Option<PersistedReceipt>,
}

struct SagaContext {
    definition: LinearSagaDefinition,
    tenant_id: TenantId,
    process_id: ProcessId,
    action_id: ActionId,
}

/// Receipt for a Parquet export managed through a durable Penelope process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsExportReceipt {
    /// Repository exported from canonical graph history.
    pub repository: RepositoryId,
    /// Worktree exported from canonical graph history.
    pub worktree: WorktreeId,
    /// Pinned generation exported.
    pub generation: GenerationId,
    /// Deterministic Parquet export summary.
    pub export: FactHistoryParquetExport,
}

/// Penelope-backed coordinator for explicit fact-history Parquet exports.
///
/// The graph store's durable-record port journals the process before the
/// restartable Parquet exporter writes any artifacts. Analytics remains
/// downstream and is never part of graph-generation publication.
pub struct PenelopeAnalyticsExporter<S> {
    store: S,
}

impl<S> PenelopeAnalyticsExporter<S>
where
    S: GraphStore + DurableRecordStore,
{
    /// Create an analytics workflow coordinator over a graph/durable-record
    /// store.
    #[must_use]
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Export one retained generation and durably correlate its completion.
    ///
    /// Repeating the exact request after interruption resumes the Parquet
    /// checkpoint or returns the recorded receipt. The same destination and
    /// generation cannot be reused with a different page size or store scope.
    ///
    /// # Errors
    /// Returns an error for an invalid page size, unavailable generation,
    /// conflicting request identity, workflow journal failure, or failed
    /// Parquet publication. An export failure leaves its prepared Penelope
    /// record available for retry.
    pub fn export_fact_history(
        &mut self,
        generation: GenerationId,
        destination: impl AsRef<Path>,
        page_size: usize,
    ) -> Result<AnalyticsExportReceipt, WorkflowError> {
        if page_size == 0 || page_size > MAX_FACT_HISTORY_PAGE_SIZE {
            return Err(WorkflowError::Verification(format!(
                "invalid fact-history page size {page_size}"
            )));
        }
        let destination = absolute_utf8_path(destination.as_ref())?;
        let manifest = self.store.manifest(generation)?;
        let request = ExportRequest {
            repository: manifest.repository,
            worktree: manifest.worktree,
            generation,
            destination,
            page_size,
        };
        let (operation_digest, request_digest) = request_digests(&request)?;
        let context = saga_context(operation_digest, request_digest)?;
        let key = record_key(operation_digest);

        let (record, encoded) = match self.store.read_record(&key)? {
            Some(encoded) => {
                let record = decode_record(&encoded)?;
                validate_record(&record, &request, request_digest, &context)?;
                (record, encoded)
            }
            None => {
                let prepared_record = prepare_record(&context, request.clone(), request_digest)?;
                let prepared_bytes = encode_record(&prepared_record)?;
                match self
                    .store
                    .compare_exchange_record(&key, None, &prepared_bytes)
                {
                    Ok(()) => (prepared_record, prepared_bytes),
                    Err(StoreError::RecordConflict(_)) => {
                        let raced_bytes = self.store.read_record(&key)?.ok_or_else(|| {
                            WorkflowError::Verification(
                                "analytics process raced creation but disappeared".to_owned(),
                            )
                        })?;
                        let raced_record = decode_record(&raced_bytes)?;
                        validate_record(&raced_record, &request, request_digest, &context)?;
                        (raced_record, raced_bytes)
                    }
                    Err(error) => return Err(WorkflowError::Store(error)),
                }
            }
        };

        if record.phase == RecordPhase::Completed {
            let export = record.receipt.ok_or_else(|| {
                WorkflowError::Verification(
                    "completed analytics process has no export receipt".to_owned(),
                )
            })?;
            return Ok(AnalyticsExportReceipt {
                repository: request.repository,
                worktree: request.worktree,
                generation,
                export: export.into(),
            });
        }

        // The Parquet manifest is the page-level checkpoint. If this returns
        // an error, keep the process prepared so the same request can retry.
        let export = export_fact_history(
            &self.store,
            generation,
            PathBuf::from(&request.destination),
            page_size,
        )
        .map_err(|error| map_export_error(&error))?;
        if !export.complete {
            return Err(WorkflowError::Verification(
                "fact-history exporter returned before completion".to_owned(),
            ));
        }

        let completed = complete_record(record, &context, export.clone().into())?;
        let replacement = encode_record(&completed)?;
        match self
            .store
            .compare_exchange_record(&key, Some(&encoded), &replacement)
        {
            Ok(()) => Ok(AnalyticsExportReceipt {
                repository: request.repository,
                worktree: request.worktree,
                generation,
                export,
            }),
            Err(StoreError::RecordConflict(_)) => {
                let current = self.store.read_record(&key)?.ok_or_else(|| {
                    WorkflowError::Verification(
                        "analytics process disappeared during completion".to_owned(),
                    )
                })?;
                let current = decode_record(&current)?;
                validate_record(&current, &request, request_digest, &context)?;
                if current.phase != RecordPhase::Completed {
                    return Err(WorkflowError::Store(StoreError::RecordConflict(key)));
                }
                let receipt = current.receipt.ok_or_else(|| {
                    WorkflowError::Verification(
                        "racing completed analytics process has no receipt".to_owned(),
                    )
                })?;
                Ok(AnalyticsExportReceipt {
                    repository: request.repository,
                    worktree: request.worktree,
                    generation,
                    export: receipt.into(),
                })
            }
            Err(error) => Err(WorkflowError::Store(error)),
        }
    }

    /// Replay and resume all prepared analytics exports using bounded journal
    /// pages. A prepared record contains enough request data to recover after
    /// restart without replaying historical graph generations.
    ///
    /// # Errors
    /// Returns an error if a workflow record is corrupt, its referenced graph
    /// generation is unavailable, or its Parquet publication cannot resume.
    pub fn recover_pending(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = 0_usize;
        loop {
            let page = self.store.records_with_prefix_page(
                RECORD_PREFIX,
                cursor.as_deref(),
                RECOVERY_PAGE_SIZE,
            )?;
            for (key, encoded) in page.records {
                let record = decode_record(&encoded)?;
                let (operation_digest, request_digest) = request_digests(&record.request)?;
                if record_key(operation_digest) != key {
                    return Err(WorkflowError::Verification(
                        "analytics workflow key does not match its persisted request".to_owned(),
                    ));
                }
                let context = saga_context(operation_digest, request_digest)?;
                validate_record(&record, &record.request, request_digest, &context)?;
                if record.phase == RecordPhase::Prepared {
                    self.export_fact_history(
                        record.request.generation,
                        &record.request.destination,
                        record.request.page_size,
                    )?;
                    recovered = recovered.saturating_add(1);
                }
            }
            let Some(next_cursor) = page.next_cursor else {
                return Ok(recovered);
            };
            cursor = Some(next_cursor);
        }
    }

    /// Return the owned store after workflow use.
    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }
}

fn absolute_utf8_path(path: &Path) -> Result<String, WorkflowError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| WorkflowError::Verification(error.to_string()))?
            .join(path)
    };
    absolute.into_os_string().into_string().map_err(|error| {
        WorkflowError::Verification(format!("export path is not valid UTF-8: {error:?}"))
    })
}

fn request_digests(
    request: &ExportRequest,
) -> Result<(ContentDigest, ContentDigest), WorkflowError> {
    let request_bytes = bincode::serialize(request)
        .map_err(|error| WorkflowError::Verification(format!("encode export request: {error}")))?;
    let request_digest = ContentDigest::sha256(&request_bytes);
    let operation_bytes = bincode::serialize(&(
        request.repository,
        request.worktree,
        request.generation,
        &request.destination,
    ))
    .map_err(|error| {
        WorkflowError::Verification(format!("encode analytics operation identity: {error}"))
    })?;
    Ok((ContentDigest::sha256(&operation_bytes), request_digest))
}

fn saga_context(
    operation_digest: ContentDigest,
    request_digest: ContentDigest,
) -> Result<SagaContext, WorkflowError> {
    let suffix = hex::encode(operation_digest.0);
    let definition = LinearSagaDefinition::new(
        DefinitionId::new(String::from("def_syntaxmesh_analytics_export"))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        DefinitionVersion::new(String::from("dfv_v1"))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        request_digest,
        vec![StepPlan {
            step_id: StepId::new(format!("stp_export_{suffix}"))
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
    request: ExportRequest,
    request_digest: ContentDigest,
) -> Result<ExportProcessRecord, WorkflowError> {
    let decision = start(
        &context.definition,
        context.tenant_id.clone(),
        context.process_id.clone(),
        context.action_id.clone(),
    )
    .map_err(|error| WorkflowError::Verification(format!("start analytics process: {error}")))?;
    let event = LinearSagaInput::Start {
        input_id: InputId::new(format!("inp_start_{}", context.process_id.as_str()))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        action_id: context.action_id.clone(),
    }
    .to_event(&context.definition);
    let record = ExportProcessRecord {
        schema_version: 1,
        request,
        request_digest,
        definition_digest: request_digest,
        events: vec![LinearSagaEventEnvelope { sequence: 0, event }],
        phase: RecordPhase::Prepared,
        receipt: None,
    };
    if replay_record(&record, context)? != decision {
        return Err(WorkflowError::Verification(
            "analytics process start event failed replay validation".to_owned(),
        ));
    }
    Ok(record)
}

fn complete_record(
    mut record: ExportProcessRecord,
    context: &SagaContext,
    receipt: PersistedReceipt,
) -> Result<ExportProcessRecord, WorkflowError> {
    let started = replay_record(&record, context)?;
    let action_id = started.projection.active_action_id.clone().ok_or_else(|| {
        WorkflowError::Verification("analytics process has no active export action".to_owned())
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
    .map_err(|error| WorkflowError::Verification(format!("complete analytics process: {error}")))?;
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
            "completed analytics process failed replay validation".to_owned(),
        ));
    }
    record.phase = RecordPhase::Completed;
    record.receipt = Some(receipt);
    Ok(record)
}

fn replay_record(
    record: &ExportProcessRecord,
    context: &SagaContext,
) -> Result<SagaDecision, WorkflowError> {
    if record.schema_version != 1 || record.definition_digest != record.request_digest {
        return Err(WorkflowError::Verification(
            "unsupported or inconsistent analytics workflow record".to_owned(),
        ));
    }
    let mut events = Vec::with_capacity(record.events.len());
    for (expected, envelope) in record.events.iter().enumerate() {
        if usize::try_from(envelope.sequence).ok() != Some(expected) {
            return Err(WorkflowError::Verification(
                "analytics workflow event sequence is not contiguous".to_owned(),
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
    .map_err(|error| WorkflowError::Verification(format!("replay analytics process: {error}")))
}

fn validate_record(
    record: &ExportProcessRecord,
    request: &ExportRequest,
    request_digest: ContentDigest,
    context: &SagaContext,
) -> Result<(), WorkflowError> {
    if record.schema_version != 1
        || &record.request != request
        || record.request_digest != request_digest
        || record.definition_digest != request_digest
    {
        return Err(WorkflowError::Verification(
            "analytics export identity was reused with different request content".to_owned(),
        ));
    }
    let decision = replay_record(record, context)?;
    let phase_is_valid = match record.phase {
        RecordPhase::Prepared => {
            decision.projection.status == SagaStatus::Running && record.receipt.is_none()
        }
        RecordPhase::Completed => {
            decision.projection.status == SagaStatus::Completed && record.receipt.is_some()
        }
    };
    if !phase_is_valid {
        return Err(WorkflowError::Verification(
            "analytics workflow phase disagrees with its replayed Penelope state".to_owned(),
        ));
    }
    Ok(())
}

fn encode_record(record: &ExportProcessRecord) -> Result<Vec<u8>, WorkflowError> {
    bincode::serialize(record)
        .map_err(|error| WorkflowError::Verification(format!("encode analytics record: {error}")))
}

fn decode_record(bytes: &[u8]) -> Result<ExportProcessRecord, WorkflowError> {
    bincode::deserialize(bytes)
        .map_err(|error| WorkflowError::Verification(format!("decode analytics record: {error}")))
}

fn record_key(operation_digest: ContentDigest) -> String {
    format!("{RECORD_PREFIX}{}", hex::encode(operation_digest.0))
}

fn map_export_error(error: &ParquetExportError) -> WorkflowError {
    WorkflowError::Verification(format!("publish analytical Parquet export: {error}"))
}

#[cfg(test)]
mod tests {
    use syntaxmesh_core::{FileId, FileVersion, GraphDelta, IndexRunId, RepositoryId, WorktreeId};
    use syntaxmesh_store::{DurableRecordStore, FileGraphStore, GraphStore};
    use tempfile::tempdir;

    use super::{PenelopeAnalyticsExporter, RECORD_PREFIX, RecordPhase, decode_record};
    use syntaxmesh_analytics_parquet::verify_fact_history_export;

    #[test]
    fn analytics_export_is_journaled_before_effect_and_retry_is_idempotent()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempdir()?;
        let store_path = temporary.path().join("graph.snapshot");
        let destination = temporary.path().join("analytics");
        let generation = syntaxmesh_core::GenerationId::derive(&[b"analytics-generation"]);
        let repository = RepositoryId::derive(&[b"analytics-repository"]);
        let worktree = WorktreeId::derive(&[b"analytics-worktree"]);
        let mut graph_store = FileGraphStore::open(&store_path)?;
        graph_store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"analytics-index-run"]),
            expected_base: None,
            next_generation: generation,
            changed_files: vec![FileVersion {
                file_id: FileId::derive(&[b"analytics-file"]),
                normalized_path: "src/lib.rs".to_owned(),
                content_hash: *blake3::hash(b"pub fn sample() {}").as_bytes(),
                size_bytes: 19,
            }],
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;
        let canonical_before = graph_store.manifest(generation)?;

        let mut exporter = PenelopeAnalyticsExporter::new(graph_store);
        std::fs::create_dir_all(&destination)?;
        let unrelated = destination.join("unrelated.txt");
        std::fs::write(&unrelated, b"not an export artifact")?;
        if exporter
            .export_fact_history(generation, &destination, 1)
            .is_ok()
        {
            return Err("Parquet exporter accepted an unrelated destination file".into());
        }
        let prepared_records = exporter.store.records_with_prefix(RECORD_PREFIX)?;
        let Some((_, prepared_bytes)) = prepared_records.first() else {
            return Err("failed export did not leave a prepared Penelope record".into());
        };
        if decode_record(prepared_bytes)?.phase != RecordPhase::Prepared {
            return Err("failed export was not left in the prepared phase".into());
        }
        std::fs::remove_file(unrelated)?;
        let failed_store = exporter.into_store();
        drop(failed_store);
        let mut recovered_exporter =
            PenelopeAnalyticsExporter::new(FileGraphStore::open(&store_path)?);
        if recovered_exporter.recover_pending()? != 1 {
            return Err("Penelope did not recover the prepared analytics export".into());
        }
        let first = recovered_exporter.export_fact_history(generation, &destination, 1)?;
        if first.repository != repository || first.worktree != worktree || !first.export.complete {
            return Err("analytics workflow returned the wrong export identity".into());
        }
        if recovered_exporter.store.manifest(generation)? != canonical_before {
            return Err("Penelope analytics workflow changed canonical graph state".into());
        }
        let completed_store = recovered_exporter.into_store();
        drop(completed_store);
        let mut stored = PenelopeAnalyticsExporter::new(FileGraphStore::open(&store_path)?);
        let retried = stored.export_fact_history(generation, &destination, 1)?;
        if retried != first {
            return Err("retry changed the completed analytics export receipt".into());
        }
        if verify_fact_history_export(&destination)?.row_count != first.export.row_count {
            return Err("verified artifact disagreed with Penelope's receipt".into());
        }
        if stored
            .export_fact_history(generation, &destination, 2)
            .is_ok()
        {
            return Err("same analytics export identity accepted a different page size".into());
        }
        Ok(())
    }
}
