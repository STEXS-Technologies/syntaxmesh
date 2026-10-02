//! Penelope journaling and recovery for downstream DuckDB materialization.

use std::path::Path;

use penelope::{
    ActionId, ActionResultObservation, ContentDigest, DefinitionId, DefinitionVersion, InputId,
    LinearSagaDefinition, LinearSagaEventEnvelope, LinearSagaInput, ProcessActionKind, ProcessId,
    RetryPolicy, SagaDecision, SagaStatus, StepId, StepPlan, TenantId, apply_action_result, replay,
    start,
};
use serde::{Deserialize, Serialize};
use syntaxmesh_analytics_duckdb::{
    FactHistoryMaterializationReceipt, FactHistoryMaterializationWatermark, FactHistoryMaterializer,
};
use syntaxmesh_core::{GenerationId, RepositoryId, WorktreeId};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

const RECORD_PREFIX: &str = "syntaxmesh.penelope.analytics-materializer.v1/";
const RECOVERY_PAGE_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum RecordPhase {
    Prepared,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterializationActionKind {
    Rebuild,
    Synchronize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum MaterializationAction {
    Rebuild {
        generation: GenerationId,
        source: String,
    },
    Synchronize {
        generation: GenerationId,
        page_size: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
enum MaterializationOperationIdentity {
    Rebuild {
        generation: GenerationId,
        source: String,
    },
    Synchronize {
        generation: GenerationId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MaterializationRequest {
    repository: RepositoryId,
    worktree: WorktreeId,
    database: String,
    action: MaterializationAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedReceipt {
    kind: MaterializationActionKind,
    repository: RepositoryId,
    worktree: WorktreeId,
    generation: GenerationId,
    generation_sequence: u64,
    changed_version_count: u64,
    source_page_count: u64,
    already_current: bool,
}

impl From<FactHistoryMaterializationReceipt> for PersistedReceipt {
    fn from(receipt: FactHistoryMaterializationReceipt) -> Self {
        Self {
            kind: MaterializationActionKind::Synchronize,
            repository: receipt.watermark.repository,
            worktree: receipt.watermark.worktree,
            generation: receipt.watermark.generation,
            generation_sequence: receipt.watermark.generation_sequence,
            changed_version_count: receipt.changed_version_count,
            source_page_count: receipt.source_page_count,
            already_current: receipt.already_current,
        }
    }
}

impl From<FactHistoryMaterializationWatermark> for PersistedReceipt {
    fn from(watermark: FactHistoryMaterializationWatermark) -> Self {
        Self {
            kind: MaterializationActionKind::Rebuild,
            repository: watermark.repository,
            worktree: watermark.worktree,
            generation: watermark.generation,
            generation_sequence: watermark.generation_sequence,
            changed_version_count: 0,
            source_page_count: 0,
            already_current: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MaterializationProcessRecord {
    schema_version: u16,
    request: MaterializationRequest,
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

/// Receipt for a Penelope-journaled DuckDB materialization effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PenelopeMaterializationReceipt {
    /// Repository materialized from canonical history.
    pub repository: RepositoryId,
    /// Worktree materialized from canonical history.
    pub worktree: WorktreeId,
    /// Completed analytical watermark.
    pub watermark: FactHistoryMaterializationWatermark,
    /// Kind of completed effect.
    pub kind: MaterializationActionKind,
    /// Changed fact versions applied; zero for full rebuilds.
    pub changed_version_count: u64,
    /// Bounded source pages consumed; zero for full rebuilds.
    pub source_page_count: u64,
    /// Whether a sync request was already fully current.
    pub already_current: bool,
}

impl From<(RepositoryId, WorktreeId, PersistedReceipt)> for PenelopeMaterializationReceipt {
    fn from((repository, worktree, receipt): (RepositoryId, WorktreeId, PersistedReceipt)) -> Self {
        Self {
            repository,
            worktree,
            watermark: FactHistoryMaterializationWatermark {
                repository: receipt.repository,
                worktree: receipt.worktree,
                generation: receipt.generation,
                generation_sequence: receipt.generation_sequence,
            },
            kind: receipt.kind,
            changed_version_count: receipt.changed_version_count,
            source_page_count: receipt.source_page_count,
            already_current: receipt.already_current,
        }
    }
}

/// Penelope-backed coordinator for explicit DuckDB rebuild and sync requests.
///
/// Prepared requests are persisted through the graph store's durable-record
/// port before DuckDB is touched. If the effect fails, the request remains
/// prepared and can be retried or recovered after reopening the coordinator.
pub struct PenelopeAnalyticsMaterializer<S> {
    store: S,
}

impl<S> PenelopeAnalyticsMaterializer<S>
where
    S: GraphStore + DurableRecordStore,
{
    /// Create a coordinator using the store for canonical reads and durable
    /// Penelope request records.
    #[must_use]
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Journal and execute a verified Parquet rebuild for one canonical
    /// generation. The DuckDB database must already have been explicitly
    /// migrated.
    ///
    /// # Errors
    /// A failed verification/rebuild leaves the Penelope request prepared for
    /// retry and does not alter canonical graph state.
    pub fn rebuild_from_export(
        &mut self,
        generation: GenerationId,
        source: impl AsRef<Path>,
        database: impl AsRef<Path>,
    ) -> Result<PenelopeMaterializationReceipt, WorkflowError> {
        let manifest = self.store.manifest(generation)?;
        let request = MaterializationRequest {
            repository: manifest.repository,
            worktree: manifest.worktree,
            database: absolute_utf8_path(database.as_ref(), "DuckDB database")?,
            action: MaterializationAction::Rebuild {
                generation,
                source: absolute_utf8_path(source.as_ref(), "Parquet export")?,
            },
        };
        self.run_request(&request)
    }

    /// Journal and execute a bounded incremental sync for one accepted
    /// generation. DuckDB's own page cursor is the recovery checkpoint; the
    /// Penelope record recovers the request and re-enters that cursor.
    ///
    /// # Errors
    /// A failed sync leaves the request prepared for retry and never affects
    /// canonical graph publication.
    pub fn synchronize_generation(
        &mut self,
        generation: GenerationId,
        database: impl AsRef<Path>,
        page_size: usize,
    ) -> Result<PenelopeMaterializationReceipt, WorkflowError> {
        let manifest = self.store.manifest(generation)?;
        let request = MaterializationRequest {
            repository: manifest.repository,
            worktree: manifest.worktree,
            database: absolute_utf8_path(database.as_ref(), "DuckDB database")?,
            action: MaterializationAction::Synchronize {
                generation,
                page_size,
            },
        };
        self.run_request(&request)
    }

    /// Replay and retry every prepared materialization request using bounded
    /// durable-record pages.
    ///
    /// # Errors
    /// Returns an error for a corrupt journal, mismatched operation key,
    /// unavailable source generation, or failed materializer effect.
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
                        "materializer journal key does not match its request".to_owned(),
                    ));
                }
                let context = saga_context(operation_digest, request_digest)?;
                validate_record(&record, &record.request, request_digest, &context)?;
                if record.phase == RecordPhase::Prepared {
                    self.run_request(&record.request)?;
                    recovered = recovered.saturating_add(1);
                }
            }
            let Some(next_cursor) = page.next_cursor else {
                return Ok(recovered);
            };
            cursor = Some(next_cursor);
        }
    }

    /// Return the graph/durable-record store after workflow use.
    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }

    fn run_request(
        &mut self,
        request: &MaterializationRequest,
    ) -> Result<PenelopeMaterializationReceipt, WorkflowError> {
        let (operation_digest, request_digest) = request_digests(request)?;
        let context = saga_context(operation_digest, request_digest)?;
        let key = record_key(operation_digest);
        let (record, encoded) = match self.store.read_record(&key)? {
            Some(encoded) => {
                let record = decode_record(&encoded)?;
                validate_record(&record, request, request_digest, &context)?;
                (record, encoded)
            }
            None => {
                let record = prepare_record(&context, request.clone(), request_digest)?;
                let encoded = encode_record(&record)?;
                match self.store.compare_exchange_record(&key, None, &encoded) {
                    Ok(()) => (record, encoded),
                    Err(StoreError::RecordConflict(_)) => {
                        let raced_bytes = self.store.read_record(&key)?.ok_or_else(|| {
                            WorkflowError::Verification(
                                "materializer request raced creation but disappeared".to_owned(),
                            )
                        })?;
                        let raced_record = decode_record(&raced_bytes)?;
                        validate_record(&raced_record, request, request_digest, &context)?;
                        (raced_record, raced_bytes)
                    }
                    Err(error) => return Err(WorkflowError::Store(error)),
                }
            }
        };

        if record.phase == RecordPhase::Completed {
            let receipt = record.receipt.ok_or_else(|| {
                WorkflowError::Verification(
                    "completed materializer request has no receipt".to_owned(),
                )
            })?;
            return Ok((request.repository, request.worktree, receipt).into());
        }

        let receipt = apply_request(&self.store, request)?;
        let completed = complete_record(record, &context, receipt)?;
        let replacement = encode_record(&completed)?;
        match self
            .store
            .compare_exchange_record(&key, Some(&encoded), &replacement)
        {
            Ok(()) => Ok((request.repository, request.worktree, receipt).into()),
            Err(StoreError::RecordConflict(_)) => {
                let current = self.store.read_record(&key)?.ok_or_else(|| {
                    WorkflowError::Verification(
                        "materializer request disappeared during completion".to_owned(),
                    )
                })?;
                let current = decode_record(&current)?;
                validate_record(&current, request, request_digest, &context)?;
                let racing_receipt = current.receipt.ok_or_else(|| {
                    WorkflowError::Verification(
                        "racing completed materializer request has no receipt".to_owned(),
                    )
                })?;
                if current.phase != RecordPhase::Completed {
                    return Err(WorkflowError::Store(StoreError::RecordConflict(key)));
                }
                Ok((request.repository, request.worktree, racing_receipt).into())
            }
            Err(error) => Err(WorkflowError::Store(error)),
        }
    }
}

fn apply_request<S: GraphStore>(
    store: &S,
    request: &MaterializationRequest,
) -> Result<PersistedReceipt, WorkflowError> {
    let mut materializer = FactHistoryMaterializer::open(&request.database).map_err(|error| {
        WorkflowError::Verification(format!("open analytics database: {error}"))
    })?;
    match &request.action {
        MaterializationAction::Rebuild { generation, source } => materializer
            .rebuild_from_graph_generation(store, *generation, source)
            .map(Into::into)
            .map_err(|error| {
                WorkflowError::Verification(format!("rebuild analytics database: {error}"))
            }),
        MaterializationAction::Synchronize {
            generation,
            page_size,
        } => materializer
            .synchronize_generation(store, *generation, *page_size)
            .map(Into::into)
            .map_err(|error| {
                WorkflowError::Verification(format!("sync analytics database: {error}"))
            }),
    }
}

fn absolute_utf8_path(path: &Path, label: &str) -> Result<String, WorkflowError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| WorkflowError::Verification(error.to_string()))?
            .join(path)
    };
    absolute.into_os_string().into_string().map_err(|error| {
        WorkflowError::Verification(format!("{label} path is not valid UTF-8: {error:?}"))
    })
}

fn request_digests(
    request: &MaterializationRequest,
) -> Result<(ContentDigest, ContentDigest), WorkflowError> {
    let request_bytes = bincode::serialize(request).map_err(|error| {
        WorkflowError::Verification(format!("encode materializer request: {error}"))
    })?;
    let request_digest = ContentDigest::sha256(&request_bytes);
    let operation = match &request.action {
        MaterializationAction::Rebuild { generation, source } => {
            MaterializationOperationIdentity::Rebuild {
                generation: *generation,
                source: source.clone(),
            }
        }
        MaterializationAction::Synchronize { generation, .. } => {
            MaterializationOperationIdentity::Synchronize {
                generation: *generation,
            }
        }
    };
    let operation_bytes = bincode::serialize(&(
        request.repository,
        request.worktree,
        &request.database,
        operation,
    ))
    .map_err(|error| {
        WorkflowError::Verification(format!("encode materializer operation identity: {error}"))
    })?;
    Ok((ContentDigest::sha256(&operation_bytes), request_digest))
}

fn saga_context(
    operation_digest: ContentDigest,
    request_digest: ContentDigest,
) -> Result<SagaContext, WorkflowError> {
    let suffix = hex::encode(operation_digest.0);
    let definition = LinearSagaDefinition::new(
        DefinitionId::new(String::from("def_syntaxmesh_analytics_materializer"))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        DefinitionVersion::new(String::from("dfv_v1"))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        request_digest,
        vec![StepPlan {
            step_id: StepId::new(format!("stp_materialize_{suffix}"))
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
    request: MaterializationRequest,
    request_digest: ContentDigest,
) -> Result<MaterializationProcessRecord, WorkflowError> {
    let decision = start(
        &context.definition,
        context.tenant_id.clone(),
        context.process_id.clone(),
        context.action_id.clone(),
    )
    .map_err(|error| WorkflowError::Verification(format!("start materializer process: {error}")))?;
    let event = LinearSagaInput::Start {
        input_id: InputId::new(format!("inp_start_{}", context.process_id.as_str()))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        action_id: context.action_id.clone(),
    }
    .to_event(&context.definition);
    let record = MaterializationProcessRecord {
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
            "materializer start event failed replay validation".to_owned(),
        ));
    }
    Ok(record)
}

fn complete_record(
    mut record: MaterializationProcessRecord,
    context: &SagaContext,
    receipt: PersistedReceipt,
) -> Result<MaterializationProcessRecord, WorkflowError> {
    let started = replay_record(&record, context)?;
    let action_id = started.projection.active_action_id.clone().ok_or_else(|| {
        WorkflowError::Verification("materializer process has no active action".to_owned())
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
    .map_err(|error| {
        WorkflowError::Verification(format!("complete materializer process: {error}"))
    })?;
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
            "completed materializer request failed replay validation".to_owned(),
        ));
    }
    record.phase = RecordPhase::Completed;
    record.receipt = Some(receipt);
    Ok(record)
}

fn replay_record(
    record: &MaterializationProcessRecord,
    context: &SagaContext,
) -> Result<SagaDecision, WorkflowError> {
    if record.schema_version != 1 || record.definition_digest != record.request_digest {
        return Err(WorkflowError::Verification(
            "unsupported or inconsistent materializer process record".to_owned(),
        ));
    }
    let mut events = Vec::with_capacity(record.events.len());
    for (expected, envelope) in record.events.iter().enumerate() {
        if usize::try_from(envelope.sequence).ok() != Some(expected) {
            return Err(WorkflowError::Verification(
                "materializer event sequence is not contiguous".to_owned(),
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
    .map_err(|error| WorkflowError::Verification(format!("replay materializer process: {error}")))
}

fn validate_record(
    record: &MaterializationProcessRecord,
    request: &MaterializationRequest,
    request_digest: ContentDigest,
    context: &SagaContext,
) -> Result<(), WorkflowError> {
    if record.schema_version != 1
        || &record.request != request
        || record.request_digest != request_digest
        || record.definition_digest != request_digest
    {
        return Err(WorkflowError::Verification(
            "materializer operation identity was reused with different request content".to_owned(),
        ));
    }
    let decision = replay_record(record, context)?;
    let valid_phase = match record.phase {
        RecordPhase::Prepared => {
            decision.projection.status == SagaStatus::Running && record.receipt.is_none()
        }
        RecordPhase::Completed => {
            decision.projection.status == SagaStatus::Completed && record.receipt.is_some()
        }
    };
    if !valid_phase {
        return Err(WorkflowError::Verification(
            "materializer record phase disagrees with replayed Penelope state".to_owned(),
        ));
    }
    Ok(())
}

fn encode_record(record: &MaterializationProcessRecord) -> Result<Vec<u8>, WorkflowError> {
    bincode::serialize(record).map_err(|error| {
        WorkflowError::Verification(format!("encode materializer record: {error}"))
    })
}

fn decode_record(bytes: &[u8]) -> Result<MaterializationProcessRecord, WorkflowError> {
    bincode::deserialize(bytes).map_err(|error| {
        WorkflowError::Verification(format!("decode materializer record: {error}"))
    })
}

fn record_key(operation_digest: ContentDigest) -> String {
    format!("{RECORD_PREFIX}{}", hex::encode(operation_digest.0))
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use syntaxmesh_analytics_duckdb::FactHistoryMaterializer;
    use syntaxmesh_analytics_parquet::export_fact_history;
    use syntaxmesh_core::{
        FileId, FileVersion, GenerationId, GraphDelta, IndexRunId, RepositoryId, WorktreeId,
    };
    use syntaxmesh_store::{DurableRecordStore, FileGraphStore, GraphStore};
    use tempfile::tempdir;

    use super::{
        MaterializationAction, MaterializationRequest, PenelopeAnalyticsMaterializer,
        RECORD_PREFIX, request_digests,
    };

    #[test]
    fn sync_operation_identity_excludes_page_size_but_request_digest_binds_it()
    -> Result<(), Box<dyn Error>> {
        let repository = RepositoryId::derive(&[b"materializer-digest-repository"]);
        let worktree = WorktreeId::derive(&[b"materializer-digest-worktree"]);
        let generation = GenerationId::derive(&[b"materializer-digest-generation"]);
        let first = MaterializationRequest {
            repository,
            worktree,
            database: "/tmp/analytics.duckdb".to_owned(),
            action: MaterializationAction::Synchronize {
                generation,
                page_size: 8,
            },
        };
        let retry_configuration = MaterializationRequest {
            action: MaterializationAction::Synchronize {
                generation,
                page_size: 16,
            },
            ..first.clone()
        };
        let (first_operation, first_request) = request_digests(&first)?;
        let (retry_operation, retry_request) = request_digests(&retry_configuration)?;
        if first_operation != retry_operation {
            return Err("page size must not create a second logical sync operation".into());
        }
        if first_request == retry_request {
            return Err("request digest must bind the recorded page-size configuration".into());
        }
        Ok(())
    }

    #[test]
    fn prepared_sync_recovers_after_materializer_bootstrap_and_rebuild()
    -> Result<(), Box<dyn Error>> {
        let directory = tempdir()?;
        let store_path = directory.path().join("graph.snapshot");
        let database_path = directory.path().join("analytics.duckdb");
        let export_path = directory.path().join("baseline-export");
        let repository = RepositoryId::derive(&[b"penelope-materializer-repository"]);
        let worktree = WorktreeId::derive(&[b"penelope-materializer-worktree"]);
        let baseline = GenerationId::derive(&[b"penelope-materializer-baseline"]);
        let current = GenerationId::derive(&[b"penelope-materializer-current"]);
        let file_id = FileId::derive(&[b"penelope-materializer-file"]);

        let mut store = FileGraphStore::open(&store_path)?;
        store.apply_delta(delta(
            repository,
            worktree,
            baseline,
            None,
            IndexRunId::derive(&[b"penelope-materializer-run-1"]),
            vec![file_version(file_id, b"before")],
            Vec::new(),
        ))?;
        store.apply_delta(delta(
            repository,
            worktree,
            current,
            Some(baseline),
            IndexRunId::derive(&[b"penelope-materializer-run-2"]),
            vec![file_version(file_id, b"after")],
            Vec::new(),
        ))?;
        export_fact_history(&store, baseline, &export_path, 8)?;

        let mut initial_coordinator = PenelopeAnalyticsMaterializer::new(store);
        let failed_before_bootstrap =
            initial_coordinator.synchronize_generation(current, &database_path, 1);
        if failed_before_bootstrap.is_ok() {
            return Err("unmigrated materializer database should leave a prepared request".into());
        }
        drop(initial_coordinator);

        FactHistoryMaterializer::migrate(&database_path)?;
        let reopened_store = FileGraphStore::open(&store_path)?;
        let mut materializer = FactHistoryMaterializer::open(&database_path)?;
        materializer.rebuild_from_graph_generation(&reopened_store, baseline, &export_path)?;
        drop(materializer);

        let mut coordinator = PenelopeAnalyticsMaterializer::new(reopened_store);
        let recovered = coordinator.recover_pending()?;
        if recovered != 1 {
            return Err(format!("expected one recovered materialization, got {recovered}").into());
        }
        let receipt = coordinator.synchronize_generation(current, &database_path, 1)?;
        if receipt.watermark.generation != current || receipt.already_current {
            return Err("recovered process must return its completed receipt".into());
        }
        let record_count = coordinator
            .store
            .records_with_prefix_page(RECORD_PREFIX, None, 8)?
            .records
            .len();
        if record_count != 1 {
            return Err(
                format!("expected one idempotent request record, got {record_count}").into(),
            );
        }

        let rebuild_database = directory.path().join("rebuild.duckdb");
        FactHistoryMaterializer::migrate(&rebuild_database)?;
        let rebuilt = coordinator.rebuild_from_export(baseline, &export_path, &rebuild_database)?;
        if rebuilt.kind != super::MaterializationActionKind::Rebuild
            || rebuilt.watermark.generation != baseline
        {
            return Err("Penelope rebuild receipt did not match the requested snapshot".into());
        }
        Ok(())
    }

    fn delta(
        repository: RepositoryId,
        worktree: WorktreeId,
        next_generation: GenerationId,
        expected_base: Option<GenerationId>,
        run_id: IndexRunId,
        changed_files: Vec<FileVersion>,
        removed_files: Vec<FileId>,
    ) -> GraphDelta {
        GraphDelta {
            repository,
            worktree,
            run_id,
            expected_base,
            next_generation,
            changed_files,
            removed_files,
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        }
    }

    fn file_version(file_id: FileId, content: &[u8]) -> FileVersion {
        FileVersion {
            file_id,
            normalized_path: "src/lib.rs".to_owned(),
            content_hash: *blake3::hash(content).as_bytes(),
            size_bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
        }
    }
}
