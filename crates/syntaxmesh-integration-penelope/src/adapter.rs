//! Restartable index-publication workflow using Penelope's pure saga engine.

use penelope::{
    ActionId, ActionResultObservation, ContentDigest, DefinitionId, DefinitionVersion, InputId,
    LinearSagaDefinition, LinearSagaEventEnvelope, LinearSagaInput, ProcessActionKind, ProcessId,
    RetryPolicy, SagaDecision, SagaStatus, StepId, StepPlan, TenantId, apply_action_result, replay,
    start,
};
use serde::{Deserialize, Serialize};
#[cfg(feature = "benchmark-instrumentation")]
use std::time::Instant;
use syntaxmesh_core::{
    AcceptanceTime, ChangeSetDelta, ConsequenceDelta, GenerationManifest, GenerationStatus,
    GraphDelta, GraphDeltaWithConsequences, GraphDeltaWithLineage, IndexRunId,
};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::{
    DurableIndexWorkflow, WorkflowError, WorkflowReceipt, WorkflowRejection, WorkflowRejectionPage,
    WorkflowRejectionReason, WorkflowStatus,
};

mod prepared_encoding;

const PROCESS_RECORD_PREFIX: &str = "syntaxmesh.penelope.index.v1/";
const REJECTION_SCAN_BATCH_SIZE: usize = 64;
const MAX_REJECTION_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum RecordPhase {
    Prepared,
    Completed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedIndexRun {
    schema_version: u16,
    run_id: IndexRunId,
    delta: GraphDelta,
    lineage: ChangeSetDelta,
    consequences: ConsequenceDelta,
    request_digest: Option<ContentDigest>,
    definition_digest: Option<ContentDigest>,
    accepted_at: Option<AcceptanceTime>,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
    accepted_generation: Option<GenerationManifest>,
    rejection_reason: Option<PersistedRejectionReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum PersistedRejectionReason {
    StaleBase {
        expected: Option<syntaxmesh_core::GenerationId>,
        actual: Option<syntaxmesh_core::GenerationId>,
    },
}

#[derive(Serialize, Deserialize)]
struct PersistedIndexRunV1 {
    schema_version: u16,
    run_id: IndexRunId,
    delta: GraphDelta,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
    accepted_generation: Option<GenerationManifest>,
}

#[derive(Serialize, Deserialize)]
struct PersistedIndexRunV2 {
    schema_version: u16,
    run_id: IndexRunId,
    delta: GraphDelta,
    accepted_at: Option<AcceptanceTime>,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
    accepted_generation: Option<GenerationManifest>,
}

#[derive(Serialize, Deserialize)]
struct PersistedIndexRunV3 {
    schema_version: u16,
    run_id: IndexRunId,
    delta: GraphDelta,
    lineage: ChangeSetDelta,
    accepted_at: Option<AcceptanceTime>,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
    accepted_generation: Option<GenerationManifest>,
}

#[derive(Serialize, Deserialize)]
struct PersistedIndexRunV4 {
    schema_version: u16,
    run_id: IndexRunId,
    delta: GraphDelta,
    lineage: ChangeSetDelta,
    consequences: ConsequenceDelta,
    accepted_at: Option<AcceptanceTime>,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
    accepted_generation: Option<GenerationManifest>,
}

#[derive(Serialize, Deserialize)]
struct PersistedIndexRunV5 {
    schema_version: u16,
    run_id: IndexRunId,
    request_digest: ContentDigest,
    accepted_at: Option<AcceptanceTime>,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
    accepted_generation: GenerationManifest,
}

#[derive(Serialize, Deserialize)]
struct PersistedIndexRunV6 {
    schema_version: u16,
    run_id: IndexRunId,
    request_digest: ContentDigest,
    definition_digest: ContentDigest,
    accepted_at: Option<AcceptanceTime>,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
    accepted_generation: GenerationManifest,
}

#[derive(Serialize, Deserialize)]
struct PersistedIndexRunV7 {
    schema_version: u16,
    run_id: IndexRunId,
    repository: syntaxmesh_core::RepositoryId,
    worktree: syntaxmesh_core::WorktreeId,
    expected_base: Option<syntaxmesh_core::GenerationId>,
    next_generation: syntaxmesh_core::GenerationId,
    request_digest: ContentDigest,
    definition_digest: ContentDigest,
    accepted_at: Option<AcceptanceTime>,
    events: Vec<LinearSagaEventEnvelope>,
    phase: RecordPhase,
    rejection_reason: PersistedRejectionReason,
}

/// Read-only counts derived from durable SyntaxMesh indexing workflow records.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PenelopeDiagnostics {
    /// Operations prepared but not yet reconciled.
    pub prepared_operations: usize,
    /// Operations completed successfully.
    pub completed_operations: usize,
    /// Operations rejected after a terminal conflict or invalid transition.
    pub rejected_operations: usize,
}

struct SagaContext {
    definition: LinearSagaDefinition,
    payload_digest: ContentDigest,
    tenant_id: TenantId,
    process_id: ProcessId,
    action_id: ActionId,
}

/// Owning Penelope publication workflow. Its operation records survive restart
/// whenever the supplied store's durable-record capability does.
pub struct PenelopeWorkflow<S> {
    store: S,
    last_decision: Option<SagaDecision>,
}

/// Borrowing publication adapter for an engine that owns its graph store.
pub struct PenelopePublisher<'store, S> {
    store: &'store mut S,
    last_decision: Option<SagaDecision>,
}

impl<S> PenelopeWorkflow<S>
where
    S: GraphStore + DurableRecordStore,
{
    /// Creates a Penelope-backed workflow over graph and durable-record ports.
    #[must_use]
    pub const fn new(store: S) -> Self {
        Self {
            store,
            last_decision: None,
        }
    }

    /// Returns the most recent replayed Penelope decision.
    #[must_use]
    pub const fn last_decision(&self) -> Option<&SagaDecision> {
        self.last_decision.as_ref()
    }

    /// Returns the underlying store after workflow use.
    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }

    /// Replays and reconciles every prepared publication record.
    ///
    /// # Errors
    /// Returns a workflow or store error when replay, compare-and-swap, or
    /// generation reconciliation fails.
    pub fn recover_pending(&mut self) -> Result<usize, WorkflowError> {
        recover_pending(self.store_mut())
    }

    const fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }
}

impl<'store, S> PenelopePublisher<'store, S>
where
    S: GraphStore + DurableRecordStore,
{
    /// Creates a publisher over mutable graph and durable-record ports.
    #[must_use]
    pub const fn new(store: &'store mut S) -> Self {
        Self {
            store,
            last_decision: None,
        }
    }

    /// Returns the latest replayed Penelope decision.
    #[must_use]
    pub const fn last_decision(&self) -> Option<&SagaDecision> {
        self.last_decision.as_ref()
    }

    /// Replays and reconciles every prepared publication record.
    ///
    /// # Errors
    /// Returns a workflow or store error when replay, compare-and-swap, or
    /// generation reconciliation fails.
    pub fn recover_pending(&mut self) -> Result<usize, WorkflowError> {
        recover_pending(self.store)
    }

    /// Inspect durable operation records without recovering or mutating them.
    ///
    /// # Errors
    /// Returns a workflow error if record enumeration or decoding fails.
    pub fn inspect(store: &S) -> Result<PenelopeDiagnostics, WorkflowError> {
        inspect_records(store)
    }

    /// Reads terminal rejection diagnostics without recovering or mutating
    /// records. Results use lexicographic run-ID order; requested page sizes
    /// are capped to keep host output bounded.
    ///
    /// # Errors
    /// Returns a workflow error if record enumeration or decoding fails.
    pub fn inspect_rejections(
        store: &S,
        after: Option<IndexRunId>,
        limit: usize,
    ) -> Result<WorkflowRejectionPage, WorkflowError> {
        inspect_rejections(store, after, limit)
    }
}

fn inspect_records<S: DurableRecordStore>(store: &S) -> Result<PenelopeDiagnostics, WorkflowError> {
    let mut diagnostics = PenelopeDiagnostics::default();
    for (_, encoded) in store.records_with_prefix(PROCESS_RECORD_PREFIX)? {
        let record = decode_record(&encoded)?;
        match record.phase {
            RecordPhase::Prepared => {
                diagnostics.prepared_operations = diagnostics.prepared_operations.saturating_add(1);
            }
            RecordPhase::Completed => {
                diagnostics.completed_operations =
                    diagnostics.completed_operations.saturating_add(1);
            }
            RecordPhase::Rejected => {
                diagnostics.rejected_operations = diagnostics.rejected_operations.saturating_add(1);
            }
        }
    }
    Ok(diagnostics)
}

fn inspect_rejections<S: DurableRecordStore>(
    store: &S,
    after: Option<IndexRunId>,
    limit: usize,
) -> Result<WorkflowRejectionPage, WorkflowError> {
    let page_size = limit.min(MAX_REJECTION_PAGE_SIZE);
    if page_size == 0 {
        return Ok(WorkflowRejectionPage {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    let mut scan_after = after.map(record_key);
    let mut items = Vec::with_capacity(page_size.saturating_add(1));
    loop {
        let page = store.records_with_prefix_page(
            PROCESS_RECORD_PREFIX,
            scan_after.as_deref(),
            REJECTION_SCAN_BATCH_SIZE,
        )?;
        for (key, encoded) in page.records {
            scan_after = Some(key);
            let record = decode_record(&encoded)?;
            if record.phase != RecordPhase::Rejected {
                continue;
            }
            items.push(WorkflowRejection {
                run_id: record.run_id,
                reason: record.rejection_reason.map_or(
                    WorkflowRejectionReason::UnknownLegacy,
                    |reason| match reason {
                        PersistedRejectionReason::StaleBase { expected, actual } => {
                            WorkflowRejectionReason::StaleBase {
                                repository: record.delta.repository,
                                worktree: record.delta.worktree,
                                expected,
                                actual,
                            }
                        }
                    },
                ),
            });
            if items.len() > page_size {
                items.truncate(page_size);
                return Ok(WorkflowRejectionPage {
                    next_cursor: items.last().map(|item| item.run_id),
                    items,
                });
            }
        }
        if page.next_cursor.is_none() {
            return Ok(WorkflowRejectionPage {
                items,
                next_cursor: None,
            });
        }
    }
}

impl<S> DurableIndexWorkflow for PenelopeWorkflow<S>
where
    S: GraphStore + DurableRecordStore,
{
    fn publish(
        &mut self,
        delta: GraphDelta,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        let (receipt, decision) = publish_index(self.store_mut(), delta, accepted_at)?;
        self.last_decision = Some(decision);
        Ok(receipt)
    }

    fn publish_with_lineage(
        &mut self,
        request: GraphDeltaWithLineage,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        let (receipt, decision) = publish_index_with_lineage(
            self.store_mut(),
            request.graph,
            request.lineage,
            accepted_at,
        )?;
        self.last_decision = Some(decision);
        Ok(receipt)
    }

    fn publish_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        let (receipt, decision) =
            publish_index_with_consequences(self.store_mut(), request, accepted_at)?;
        self.last_decision = Some(decision);
        Ok(receipt)
    }
}

impl<S> DurableIndexWorkflow for PenelopePublisher<'_, S>
where
    S: GraphStore + DurableRecordStore,
{
    fn publish(
        &mut self,
        delta: GraphDelta,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        let (receipt, decision) = publish_index(self.store, delta, accepted_at)?;
        self.last_decision = Some(decision);
        Ok(receipt)
    }

    fn publish_with_lineage(
        &mut self,
        request: GraphDeltaWithLineage,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        let (receipt, decision) =
            publish_index_with_lineage(self.store, request.graph, request.lineage, accepted_at)?;
        self.last_decision = Some(decision);
        Ok(receipt)
    }

    fn publish_with_consequences(
        &mut self,
        request: GraphDeltaWithConsequences,
        accepted_at: AcceptanceTime,
    ) -> Result<WorkflowReceipt, WorkflowError> {
        let (receipt, decision) =
            publish_index_with_consequences(self.store, request, accepted_at)?;
        self.last_decision = Some(decision);
        Ok(receipt)
    }
}

fn publish_index<S>(
    store: &mut S,
    delta: GraphDelta,
    accepted_at: AcceptanceTime,
) -> Result<(WorkflowReceipt, SagaDecision), WorkflowError>
where
    S: GraphStore + DurableRecordStore,
{
    publish_index_with_lineage(store, delta, ChangeSetDelta::default(), accepted_at)
}

fn publish_index_with_lineage<S>(
    store: &mut S,
    delta: GraphDelta,
    lineage: ChangeSetDelta,
    accepted_at: AcceptanceTime,
) -> Result<(WorkflowReceipt, SagaDecision), WorkflowError>
where
    S: GraphStore + DurableRecordStore,
{
    publish_index_with_consequences(
        store,
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: delta,
                lineage,
            },
            consequences: ConsequenceDelta::default(),
        },
        accepted_at,
    )
}

fn publish_index_with_consequences<S>(
    store: &mut S,
    request: GraphDeltaWithConsequences,
    accepted_at: AcceptanceTime,
) -> Result<(WorkflowReceipt, SagaDecision), WorkflowError>
where
    S: GraphStore + DurableRecordStore,
{
    #[cfg(feature = "benchmark-instrumentation")]
    let profile = std::env::var_os("SYNTAXMESH_PENELOPE_PROFILE").is_some();
    let delta = request.publication.graph;
    let lineage = request.publication.lineage;
    let consequences = request.consequences;
    #[cfg(feature = "benchmark-instrumentation")]
    let recovery_started = Instant::now();
    recover_pending(store)?;
    #[cfg(feature = "benchmark-instrumentation")]
    report_publication_stage(profile, "publish_recovery", recovery_started);

    let key = record_key(delta.run_id);
    #[cfg(feature = "benchmark-instrumentation")]
    let record_started = Instant::now();
    let existing = store.read_record(&key)?;
    let mut completed_legacy_digests = None;
    let (record, encoded_record) = if let Some(encoded) = existing {
        let record = decode_record(&encoded)?;
        if !record_matches_request(&record, &delta, &lineage, &consequences)? {
            if record.lineage.is_empty()
                && lineage.is_empty()
                && record.consequences.is_empty()
                && consequences.is_empty()
                && let Some(manifest) = completed_noop_replay(store, &record, &delta)?
            {
                let decision = replay_record(&record)?;
                return Ok((receipt(record.run_id, manifest), decision));
            }
            return Err(WorkflowError::Verification(
                "Penelope run ID was reused for a different graph delta".to_owned(),
            ));
        }
        if (1..=4).contains(&record.schema_version)
            && record.phase == RecordPhase::Completed
            && record.accepted_generation.is_some()
        {
            let request_digest =
                saga_context(&delta, &lineage, &consequences, true)?.payload_digest;
            let definition_digest = saga_context(
                &record.delta,
                &record.lineage,
                &record.consequences,
                record.schema_version >= 4,
            )?
            .payload_digest;
            completed_legacy_digests = Some((request_digest, definition_digest));
        }
        (record, encoded)
    } else {
        let record = prepare_record(delta, lineage, consequences, accepted_at)?;
        let encoded = encode_record(&record)?;
        match store.compare_exchange_record(&key, None, &encoded) {
            Ok(()) => (record, encoded),
            Err(StoreError::RecordConflict(_)) => {
                let actual = store.read_record(&key)?.ok_or_else(|| {
                    WorkflowError::Verification(
                        "Penelope record raced creation but disappeared".to_owned(),
                    )
                })?;
                let actual_record = decode_record(&actual)?;
                if actual_record.delta != record.delta
                    || actual_record.lineage != record.lineage
                    || actual_record.consequences != record.consequences
                {
                    return Err(WorkflowError::Verification(
                        "Penelope run ID was concurrently reused for a different delta".to_owned(),
                    ));
                }
                (actual_record, actual)
            }
            Err(error) => return Err(WorkflowError::Store(error)),
        }
    };
    #[cfg(feature = "benchmark-instrumentation")]
    report_publication_stage(profile, "record_read_prepare_cas", record_started);

    let (record, encoded_record) = if (1..=4).contains(&record.schema_version)
        && record.phase == RecordPhase::Completed
        && record.request_digest.is_none()
        && record.accepted_generation.is_some()
    {
        let mut compacted = record;
        let (request_digest, definition_digest) = completed_legacy_digests.ok_or_else(|| {
            WorkflowError::Verification(
                "completed legacy Penelope retry is missing its digests".to_owned(),
            )
        })?;
        compacted.request_digest = Some(request_digest);
        compacted.definition_digest = Some(definition_digest);
        compacted.schema_version = 6;
        let compacted_bytes = encode_record(&compacted)?;
        match store.compare_exchange_record(&key, Some(&encoded_record), &compacted_bytes) {
            Ok(()) => (compacted, compacted_bytes),
            Err(StoreError::RecordConflict(_)) => {
                let actual = store.read_record(&key)?.ok_or_else(|| {
                    WorkflowError::Verification(
                        "Penelope record disappeared during completed-record compaction".to_owned(),
                    )
                })?;
                let actual_record = decode_record(&actual)?;
                if actual_record.phase != RecordPhase::Completed
                    || actual_record.request_digest != Some(request_digest)
                {
                    return Err(WorkflowError::Store(StoreError::RecordConflict(key)));
                }
                (actual_record, actual)
            }
            Err(error) => return Err(WorkflowError::Store(error)),
        }
    } else {
        (record, encoded_record)
    };
    if record.phase == RecordPhase::Completed {
        let accepted_generation = record.accepted_generation.clone().ok_or_else(|| {
            WorkflowError::Verification("completed Penelope record has no generation".to_owned())
        })?;
        let manifest = store
            .current_generation(record.delta.repository, record.delta.worktree)?
            .filter(|current| current.generation == accepted_generation.generation)
            .unwrap_or(accepted_generation);
        let decision = replay_record(&record)?;
        return Ok((receipt(record.run_id, manifest), decision));
    }
    if record.phase == RecordPhase::Rejected {
        let (expected, actual) = match record.rejection_reason {
            Some(PersistedRejectionReason::StaleBase { expected, actual }) => (expected, actual),
            None => (
                record.delta.expected_base,
                store
                    .current_generation(record.delta.repository, record.delta.worktree)?
                    .map(|manifest| manifest.generation),
            ),
        };
        return Err(WorkflowError::Store(StoreError::StaleBase {
            expected,
            actual,
        }));
    }
    #[cfg(feature = "benchmark-instrumentation")]
    let graph_started = Instant::now();
    let manifest = match reconcile_and_publish(
        store,
        &record.delta,
        &record.lineage,
        &record.consequences,
        record.accepted_at,
    ) {
        Ok(manifest) => manifest,
        Err(WorkflowError::Store(error @ StoreError::StaleBase { expected, actual })) => {
            reject_prepared_record(
                store,
                &key,
                &encoded_record,
                record,
                PersistedRejectionReason::StaleBase { expected, actual },
            )?;
            return Err(WorkflowError::Store(error));
        }
        Err(error) => return Err(error),
    };
    #[cfg(feature = "benchmark-instrumentation")]
    report_publication_stage(profile, "graph_commit", graph_started);

    #[cfg(feature = "benchmark-instrumentation")]
    let completion_started = Instant::now();
    let (completed, decision) = complete_record(record, manifest)?;
    let completed_bytes = encode_record(&completed)?;
    match store.compare_exchange_record(&key, Some(&encoded_record), &completed_bytes) {
        Ok(()) => {
            #[cfg(feature = "benchmark-instrumentation")]
            report_publication_stage(profile, "completion_record_cas", completion_started);
            Ok((
                receipt(
                    completed.run_id,
                    completed.accepted_generation.ok_or_else(|| {
                        WorkflowError::Verification(
                            "completed Penelope record lost its generation".to_owned(),
                        )
                    })?,
                ),
                decision,
            ))
        }
        Err(StoreError::RecordConflict(_)) => {
            let actual = store.read_record(&key)?.ok_or_else(|| {
                WorkflowError::Verification("completed Penelope record disappeared".to_owned())
            })?;
            let actual_record = decode_record(&actual)?;
            if actual_record.phase != RecordPhase::Completed {
                return Err(WorkflowError::Store(StoreError::RecordConflict(key)));
            }
            let recovered_manifest =
                actual_record.accepted_generation.clone().ok_or_else(|| {
                    WorkflowError::Verification(
                        "completed Penelope record has no generation".to_owned(),
                    )
                })?;
            Ok((
                receipt(actual_record.run_id, recovered_manifest),
                replay_record(&actual_record)?,
            ))
        }
        Err(error) => Err(WorkflowError::Store(error)),
    }
}

#[cfg(feature = "benchmark-instrumentation")]
fn report_publication_stage(enabled: bool, stage: &str, started: Instant) {
    if enabled {
        eprintln!(
            "penelope_stage stage={stage} elapsed_us={}",
            started.elapsed().as_micros()
        );
    }
}

fn completed_noop_replay<S>(
    store: &S,
    record: &PersistedIndexRun,
    delta: &GraphDelta,
) -> Result<Option<GenerationManifest>, WorkflowError>
where
    S: GraphStore,
{
    let no_mutations = delta.changed_files.is_empty()
        && delta.removed_files.is_empty()
        && delta.upsert_provenance.is_empty()
        && delta.upsert_nodes.is_empty()
        && delta.upsert_edges.is_empty()
        && delta.remove_nodes.is_empty()
        && delta.remove_edges.is_empty();
    if record.phase != RecordPhase::Completed
        || !no_mutations
        || !record.consequences.is_empty()
        || record.run_id != delta.run_id
        || record.delta.repository != delta.repository
        || record.delta.worktree != delta.worktree
        || record.delta.next_generation != delta.next_generation
    {
        return Ok(None);
    }
    let Some(accepted) = record.accepted_generation.as_ref() else {
        return Ok(None);
    };
    let current = store.current_generation(delta.repository, delta.worktree)?;
    Ok(current.filter(|manifest| {
        manifest.generation == accepted.generation && manifest.graph_root == accepted.graph_root
    }))
}

fn record_matches_request(
    record: &PersistedIndexRun,
    delta: &GraphDelta,
    lineage: &ChangeSetDelta,
    consequences: &ConsequenceDelta,
) -> Result<bool, WorkflowError> {
    if let Some(stored_digest) = record.request_digest {
        let context = saga_context(delta, lineage, consequences, true)?;
        return Ok(context.payload_digest == stored_digest);
    }
    Ok(
        record.delta == *delta
            && record.lineage == *lineage
            && record.consequences == *consequences,
    )
}

fn recover_pending<S>(store: &mut S) -> Result<usize, WorkflowError>
where
    S: GraphStore + DurableRecordStore,
{
    let records = store
        .records_with_prefix(PROCESS_RECORD_PREFIX)?
        .into_iter()
        .map(|(key, encoded)| {
            let record = decode_record(&encoded)?;
            Ok((key, encoded, record))
        })
        .collect::<Result<Vec<_>, WorkflowError>>()?;
    let mut pending = records
        .into_iter()
        .filter(|(_, _, record)| record.phase == RecordPhase::Prepared)
        .collect::<Vec<_>>();
    let mut recovered = 0_usize;
    while !pending.is_empty() {
        let mut eligible_position = None;
        for (position, (_, _, record)) in pending.iter().enumerate() {
            let current =
                store.current_generation(record.delta.repository, record.delta.worktree)?;
            if current.is_some_and(|manifest| manifest.generation == record.delta.next_generation) {
                eligible_position = Some(position);
                break;
            }
        }
        if eligible_position.is_none() {
            for (position, (_, _, record)) in pending.iter().enumerate() {
                let current =
                    store.current_generation(record.delta.repository, record.delta.worktree)?;
                if current.map(|manifest| manifest.generation) == record.delta.expected_base {
                    eligible_position = Some(position);
                    break;
                }
            }
        }
        let position = eligible_position.unwrap_or(0);
        let (key, encoded, record) = pending.remove(position);
        let manifest = match reconcile_and_publish(
            store,
            &record.delta,
            &record.lineage,
            &record.consequences,
            record.accepted_at,
        ) {
            Ok(manifest) => manifest,
            Err(WorkflowError::Store(StoreError::StaleBase { expected, actual })) => {
                reject_prepared_record(
                    store,
                    &key,
                    &encoded,
                    record,
                    PersistedRejectionReason::StaleBase { expected, actual },
                )?;
                recovered = recovered.saturating_add(1);
                continue;
            }
            Err(error) => return Err(error),
        };
        let (completed, _) = complete_record(record, manifest)?;
        let replacement = encode_record(&completed)?;
        match store.compare_exchange_record(&key, Some(&encoded), &replacement) {
            Ok(()) => recovered = recovered.saturating_add(1),
            Err(StoreError::RecordConflict(_)) => {
                let latest = store.read_record(&key)?.ok_or_else(|| {
                    WorkflowError::Verification("recovering Penelope record disappeared".to_owned())
                })?;
                if decode_record(&latest)?.phase != RecordPhase::Completed {
                    return Err(WorkflowError::Store(StoreError::RecordConflict(key)));
                }
            }
            Err(error) => return Err(WorkflowError::Store(error)),
        }
    }
    Ok(recovered)
}

fn reject_prepared_record<S>(
    store: &mut S,
    key: &str,
    encoded: &[u8],
    record: PersistedIndexRun,
    reason: PersistedRejectionReason,
) -> Result<(), WorkflowError>
where
    S: DurableRecordStore,
{
    let (rejected, _) = reject_record(record, reason)?;
    let replacement = encode_record(&rejected)?;
    match store.compare_exchange_record(key, Some(encoded), &replacement) {
        Ok(()) => Ok(()),
        Err(StoreError::RecordConflict(_)) => {
            let latest = store.read_record(key)?.ok_or_else(|| {
                WorkflowError::Verification("rejected Penelope record disappeared".to_owned())
            })?;
            if matches!(
                decode_record(&latest)?.phase,
                RecordPhase::Rejected | RecordPhase::Completed
            ) {
                Ok(())
            } else {
                Err(WorkflowError::Store(StoreError::RecordConflict(
                    key.to_owned(),
                )))
            }
        }
        Err(error) => Err(WorkflowError::Store(error)),
    }
}

fn reject_record(
    mut record: PersistedIndexRun,
    reason: PersistedRejectionReason,
) -> Result<(PersistedIndexRun, SagaDecision), WorkflowError> {
    let context = saga_context(
        &record.delta,
        &record.lineage,
        &record.consequences,
        record.schema_version >= 4,
    )?;
    let started = replay_record(&record)?;
    let action_id = started.projection.active_action_id.clone().ok_or_else(|| {
        WorkflowError::Verification(
            "Penelope start event has no active publication action".to_owned(),
        )
    })?;
    let observation = ActionResultObservation::terminal_failure(action_id);
    let decided = apply_action_result(
        &context.definition,
        &started.projection,
        context.tenant_id.clone(),
        context.process_id.clone(),
        &observation,
        None,
    )
    .map_err(|error| {
        WorkflowError::Verification(format!("reject Penelope index process: {error}"))
    })?;
    let rejected_event = LinearSagaInput::ActionResult {
        input_id: InputId::new(format!("inp_rejected_{}", record.run_id.0.to_hex()))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        observation,
        next_action_id: None,
    }
    .to_event(&context.definition);
    record.events.push(LinearSagaEventEnvelope {
        sequence: u64::try_from(record.events.len())
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        event: rejected_event,
    });
    let decision = replay_record(&record)?;
    if decision != decided || decision.projection.status != SagaStatus::Escalated {
        return Err(WorkflowError::Verification(
            "Penelope did not replay stale publication as a terminal rejection".to_owned(),
        ));
    }
    record.phase = RecordPhase::Rejected;
    record.request_digest = Some(context.payload_digest);
    record.definition_digest = Some(context.payload_digest);
    record.rejection_reason = Some(reason);
    record.schema_version = 7;
    Ok((record, decision))
}

fn reconcile_and_publish<S>(
    store: &mut S,
    delta: &GraphDelta,
    lineage: &ChangeSetDelta,
    consequences: &ConsequenceDelta,
    accepted_at: Option<AcceptanceTime>,
) -> Result<GenerationManifest, WorkflowError>
where
    S: GraphStore,
{
    let current = store.current_generation(delta.repository, delta.worktree)?;
    if let Some(manifest) = current.as_ref()
        && manifest.generation == delta.next_generation
        && manifest.parent == delta.expected_base
    {
        return (lineage_matches_generation(store, manifest.generation, lineage)?
            && consequence_matches_generation(store, manifest.generation, consequences)?)
        .then(|| manifest.clone())
        .ok_or_else(|| {
            WorkflowError::Store(StoreError::InvalidDelta(
                "recovered generation has a different accepted lineage delta".to_owned(),
            ))
        });
    }
    match store.apply_delta_with_consequences(
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: delta.clone(),
                lineage: lineage.clone(),
            },
            consequences: consequences.clone(),
        },
        accepted_at,
    ) {
        Ok(manifest) => Ok(manifest),
        Err(StoreError::StaleBase { .. }) => {
            let actual_manifest = store.current_generation(delta.repository, delta.worktree)?;
            let actual = actual_manifest.as_ref().map(|manifest| manifest.generation);
            actual_manifest
                .as_ref()
                .filter(|manifest| {
                    manifest.generation == delta.next_generation
                        && manifest.parent == delta.expected_base
                })
                .map_or_else(
                    || {
                        Err(WorkflowError::Store(StoreError::StaleBase {
                            expected: delta.expected_base,
                            actual,
                        }))
                    },
                    |manifest| {
                        if lineage_matches_generation(store, manifest.generation, lineage)?
                            && consequence_matches_generation(
                                store,
                                manifest.generation,
                                consequences,
                            )?
                        {
                            Ok(manifest.clone())
                        } else {
                            Err(WorkflowError::Store(StoreError::InvalidDelta(
                                "concurrent generation has a different accepted lineage delta"
                                    .to_owned(),
                            )))
                        }
                    },
                )
        }
        Err(error) => Err(WorkflowError::Store(error)),
    }
}

fn consequence_matches_generation<S: GraphStore>(
    store: &S,
    generation: syntaxmesh_core::GenerationId,
    expected: &ConsequenceDelta,
) -> Result<bool, WorkflowError> {
    Ok(store
        .generation_consequence_history()?
        .into_iter()
        .find(|entry| entry.generation == generation)
        .is_some_and(|entry| entry.delta == *expected))
}

fn lineage_matches_generation<S: GraphStore>(
    store: &S,
    generation: syntaxmesh_core::GenerationId,
    expected: &ChangeSetDelta,
) -> Result<bool, WorkflowError> {
    Ok(store
        .generation_lineage_history()?
        .into_iter()
        .find(|entry| entry.generation == generation)
        .is_some_and(|entry| entry.delta == *expected))
}

fn complete_record(
    mut record: PersistedIndexRun,
    manifest: GenerationManifest,
) -> Result<(PersistedIndexRun, SagaDecision), WorkflowError> {
    let context = saga_context(
        &record.delta,
        &record.lineage,
        &record.consequences,
        record.schema_version >= 4,
    )?;
    let started = replay_record(&record)?;
    let action_id = started.projection.active_action_id.clone().ok_or_else(|| {
        WorkflowError::Verification(
            "Penelope start event has no active publication action".to_owned(),
        )
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
        WorkflowError::Verification(format!("apply Penelope publication result: {error}"))
    })?;
    let action_input = LinearSagaInput::ActionResult {
        input_id: InputId::new(format!("inp_published_{}", record.run_id.0.to_hex()))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        observation,
        next_action_id: None,
    }
    .to_event(&context.definition);
    record.events.push(LinearSagaEventEnvelope {
        sequence: u64::try_from(record.events.len())
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        event: action_input,
    });
    let decision = replay_record(&record)?;
    if decision != decided || decision.projection.status != SagaStatus::Completed {
        return Err(WorkflowError::Verification(
            "Penelope did not complete the accepted index process".to_owned(),
        ));
    }
    record.phase = RecordPhase::Completed;
    record.accepted_generation = Some(manifest);
    record.request_digest = Some(
        saga_context(&record.delta, &record.lineage, &record.consequences, true)?.payload_digest,
    );
    record.definition_digest = Some(context.payload_digest);
    record.schema_version = 6;
    Ok((record, decision))
}

fn prepare_record(
    delta: GraphDelta,
    lineage: ChangeSetDelta,
    consequences: ConsequenceDelta,
    accepted_at: AcceptanceTime,
) -> Result<PersistedIndexRun, WorkflowError> {
    let context = saga_context(&delta, &lineage, &consequences, true)?;
    let started = start(
        &context.definition,
        context.tenant_id.clone(),
        context.process_id.clone(),
        context.action_id.clone(),
    )
    .map_err(|error| {
        WorkflowError::Verification(format!("start Penelope index process: {error}"))
    })?;
    let start_event = LinearSagaInput::Start {
        input_id: InputId::new(format!("inp_start_{}", delta.run_id.0.to_hex()))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        action_id: context.action_id,
    }
    .to_event(&context.definition);
    let record = PersistedIndexRun {
        schema_version: 4,
        run_id: delta.run_id,
        delta,
        lineage,
        consequences,
        request_digest: None,
        definition_digest: None,
        accepted_at: Some(accepted_at),
        events: vec![LinearSagaEventEnvelope {
            sequence: 0,
            event: start_event,
        }],
        phase: RecordPhase::Prepared,
        accepted_generation: None,
        rejection_reason: None,
    };
    if replay_record(&record)? != started {
        return Err(WorkflowError::Verification(
            "Penelope start event did not replay to its start decision".to_owned(),
        ));
    }
    Ok(record)
}

fn replay_record(record: &PersistedIndexRun) -> Result<SagaDecision, WorkflowError> {
    if !matches!(record.schema_version, 1..=7) {
        return Err(WorkflowError::Verification(format!(
            "unsupported Penelope index-record version {}",
            record.schema_version
        )));
    }
    let context = if (5..=7).contains(&record.schema_version) {
        let digest = record
            .definition_digest
            .or(record.request_digest)
            .ok_or_else(|| {
                WorkflowError::Verification(
                    "compact Penelope record has no definition digest".to_owned(),
                )
            })?;
        saga_context_from_digest(record.run_id, digest)?
    } else {
        saga_context(
            &record.delta,
            &record.lineage,
            &record.consequences,
            record.schema_version >= 4,
        )?
    };
    let mut events = Vec::with_capacity(record.events.len());
    for (expected, envelope) in record.events.iter().enumerate() {
        if usize::try_from(envelope.sequence).ok() != Some(expected) {
            return Err(WorkflowError::Verification(
                "Penelope index-event sequence is not contiguous".to_owned(),
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
    .map_err(|error| WorkflowError::Verification(format!("replay Penelope index process: {error}")))
}

fn saga_context(
    delta: &GraphDelta,
    lineage: &ChangeSetDelta,
    consequences: &ConsequenceDelta,
    include_consequences: bool,
) -> Result<SagaContext, WorkflowError> {
    let payload_value = GraphDeltaWithLineage {
        graph: delta.clone(),
        lineage: lineage.clone(),
    };
    let payload = if include_consequences {
        bincode::serialize(&GraphDeltaWithConsequences {
            publication: payload_value,
            consequences: consequences.clone(),
        })
    } else {
        bincode::serialize(&payload_value)
    }
    .map_err(|error| WorkflowError::Verification(format!("encode graph delta: {error}")))?;
    let payload_digest = ContentDigest::sha256(&payload);
    saga_context_from_digest(delta.run_id, payload_digest)
}

fn saga_context_from_digest(
    run_id: IndexRunId,
    payload_digest: ContentDigest,
) -> Result<SagaContext, WorkflowError> {
    let suffix = run_id.0.to_hex();
    let definition_id = DefinitionId::new(String::from("def_syntaxmesh_index"))
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    let definition_version = DefinitionVersion::new(String::from("dfv_v1"))
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    let step_id = StepId::new(format!("stp_publish_{suffix}"))
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    let tenant_id = TenantId::new(String::from("tnt_syntaxmesh"))
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    let process_id = ProcessId::new(format!("prc_{suffix}"))
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    let action_id = ActionId::new(format!("act_{suffix}"))
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    let definition = LinearSagaDefinition::new(
        definition_id,
        definition_version,
        payload_digest,
        vec![StepPlan {
            step_id,
            action_kind: ProcessActionKind::CanonicalCommand,
            payload_digest,
            retry_policy: RetryPolicy::no_retry(),
            compensation: None,
        }],
    );
    Ok(SagaContext {
        definition,
        payload_digest,
        tenant_id,
        process_id,
        action_id,
    })
}

fn record_key(run_id: IndexRunId) -> String {
    format!("{PROCESS_RECORD_PREFIX}{}", run_id.0.to_hex())
}

fn encode_record(record: &PersistedIndexRun) -> Result<Vec<u8>, WorkflowError> {
    let payload = match record.schema_version {
        1..=4 => return prepared_encoding::encode(record),
        5 => {
            let accepted_generation = record.accepted_generation.clone().ok_or_else(|| {
                WorkflowError::Verification(
                    "compact Penelope record has no accepted generation".to_owned(),
                )
            })?;
            let request_digest = record.request_digest.ok_or_else(|| {
                WorkflowError::Verification(
                    "compact Penelope record has no request digest".to_owned(),
                )
            })?;
            if record.phase != RecordPhase::Completed {
                return Err(WorkflowError::Verification(
                    "compact Penelope record is not terminal".to_owned(),
                ));
            }
            bincode::serialize(&PersistedIndexRunV5 {
                schema_version: 5,
                run_id: record.run_id,
                request_digest,
                accepted_at: record.accepted_at,
                events: record.events.clone(),
                phase: record.phase,
                accepted_generation,
            })
        }
        6 => {
            let accepted_generation = record.accepted_generation.clone().ok_or_else(|| {
                WorkflowError::Verification(
                    "compact Penelope record has no accepted generation".to_owned(),
                )
            })?;
            let request_digest = record.request_digest.ok_or_else(|| {
                WorkflowError::Verification(
                    "compact Penelope record has no request digest".to_owned(),
                )
            })?;
            let definition_digest = record.definition_digest.ok_or_else(|| {
                WorkflowError::Verification(
                    "compact Penelope record has no definition digest".to_owned(),
                )
            })?;
            if record.phase != RecordPhase::Completed {
                return Err(WorkflowError::Verification(
                    "compact Penelope record is not completed".to_owned(),
                ));
            }
            bincode::serialize(&PersistedIndexRunV6 {
                schema_version: 6,
                run_id: record.run_id,
                request_digest,
                definition_digest,
                accepted_at: record.accepted_at,
                events: record.events.clone(),
                phase: record.phase,
                accepted_generation,
            })
        }
        7 => {
            let rejection_reason = record.rejection_reason.ok_or_else(|| {
                WorkflowError::Verification(
                    "compact Penelope rejection record has no reason".to_owned(),
                )
            })?;
            if record.phase != RecordPhase::Rejected {
                return Err(WorkflowError::Verification(
                    "compact Penelope v7 record is not rejected".to_owned(),
                ));
            }
            let request_digest = record.request_digest.ok_or_else(|| {
                WorkflowError::Verification(
                    "compact Penelope rejection record has no request digest".to_owned(),
                )
            })?;
            let definition_digest = record.definition_digest.ok_or_else(|| {
                WorkflowError::Verification(
                    "compact Penelope rejection record has no definition digest".to_owned(),
                )
            })?;
            bincode::serialize(&PersistedIndexRunV7 {
                schema_version: 7,
                run_id: record.run_id,
                repository: record.delta.repository,
                worktree: record.delta.worktree,
                expected_base: record.delta.expected_base,
                next_generation: record.delta.next_generation,
                request_digest,
                definition_digest,
                accepted_at: record.accepted_at,
                events: record.events.clone(),
                phase: record.phase,
                rejection_reason,
            })
        }
        version => {
            return Err(WorkflowError::Verification(format!(
                "unsupported Penelope index-record version {version}"
            )));
        }
    }
    .map_err(|error| WorkflowError::Verification(format!("encode Penelope record: {error}")))?;
    Ok(payload)
}

fn decode_record(payload: &[u8]) -> Result<PersistedIndexRun, WorkflowError> {
    let version = bincode::deserialize::<u16>(payload).map_err(|error| {
        WorkflowError::Verification(format!("decode Penelope record version: {error}"))
    })?;
    match version {
        1 => {
            let legacy: PersistedIndexRunV1 = bincode::deserialize(payload).map_err(|error| {
                WorkflowError::Verification(format!("decode legacy Penelope record: {error}"))
            })?;
            Ok(PersistedIndexRun {
                schema_version: legacy.schema_version,
                run_id: legacy.run_id,
                delta: legacy.delta,
                lineage: ChangeSetDelta::default(),
                consequences: ConsequenceDelta::default(),
                request_digest: None,
                definition_digest: None,
                accepted_at: None,
                events: legacy.events,
                phase: legacy.phase,
                accepted_generation: legacy.accepted_generation,
                rejection_reason: None,
            })
        }
        2 => {
            let legacy: PersistedIndexRunV2 = bincode::deserialize(payload).map_err(|error| {
                WorkflowError::Verification(format!("decode Penelope v2 record: {error}"))
            })?;
            Ok(PersistedIndexRun {
                schema_version: legacy.schema_version,
                run_id: legacy.run_id,
                delta: legacy.delta,
                lineage: ChangeSetDelta::default(),
                consequences: ConsequenceDelta::default(),
                request_digest: None,
                definition_digest: None,
                accepted_at: legacy.accepted_at,
                events: legacy.events,
                phase: legacy.phase,
                accepted_generation: legacy.accepted_generation,
                rejection_reason: None,
            })
        }
        3 => {
            let legacy: PersistedIndexRunV3 = bincode::deserialize(payload).map_err(|error| {
                WorkflowError::Verification(format!("decode Penelope v3 record: {error}"))
            })?;
            Ok(PersistedIndexRun {
                schema_version: legacy.schema_version,
                run_id: legacy.run_id,
                delta: legacy.delta,
                lineage: legacy.lineage,
                consequences: ConsequenceDelta::default(),
                request_digest: None,
                definition_digest: None,
                accepted_at: legacy.accepted_at,
                events: legacy.events,
                phase: legacy.phase,
                accepted_generation: legacy.accepted_generation,
                rejection_reason: None,
            })
        }
        4 => {
            let legacy: PersistedIndexRunV4 = bincode::deserialize(payload).map_err(|error| {
                WorkflowError::Verification(format!("decode Penelope v4 record: {error}"))
            })?;
            Ok(PersistedIndexRun {
                schema_version: 4,
                run_id: legacy.run_id,
                delta: legacy.delta,
                lineage: legacy.lineage,
                consequences: legacy.consequences,
                request_digest: None,
                definition_digest: None,
                accepted_at: legacy.accepted_at,
                events: legacy.events,
                phase: legacy.phase,
                accepted_generation: legacy.accepted_generation,
                rejection_reason: None,
            })
        }
        5 => {
            let compact: PersistedIndexRunV5 = bincode::deserialize(payload).map_err(|error| {
                WorkflowError::Verification(format!("decode compact Penelope v5 record: {error}"))
            })?;
            if compact.phase != RecordPhase::Completed {
                return Err(WorkflowError::Verification(
                    "compact Penelope v5 record is not terminal".to_owned(),
                ));
            }
            let generation = &compact.accepted_generation;
            Ok(PersistedIndexRun {
                schema_version: 5,
                run_id: compact.run_id,
                delta: GraphDelta {
                    repository: generation.repository,
                    worktree: generation.worktree,
                    run_id: compact.run_id,
                    expected_base: generation.parent,
                    next_generation: generation.generation,
                    changed_files: Vec::new(),
                    removed_files: Vec::new(),
                    upsert_provenance: Vec::new(),
                    upsert_nodes: Vec::new(),
                    upsert_edges: Vec::new(),
                    remove_nodes: Vec::new(),
                    remove_edges: Vec::new(),
                },
                lineage: ChangeSetDelta::default(),
                consequences: ConsequenceDelta::default(),
                request_digest: Some(compact.request_digest),
                definition_digest: Some(compact.request_digest),
                accepted_at: compact.accepted_at,
                events: compact.events,
                phase: compact.phase,
                accepted_generation: Some(compact.accepted_generation),
                rejection_reason: None,
            })
        }
        6 => {
            let compact: PersistedIndexRunV6 = bincode::deserialize(payload).map_err(|error| {
                WorkflowError::Verification(format!("decode compact Penelope v6 record: {error}"))
            })?;
            if compact.phase != RecordPhase::Completed {
                return Err(WorkflowError::Verification(
                    "compact Penelope v6 record is not completed".to_owned(),
                ));
            }
            let generation = &compact.accepted_generation;
            Ok(PersistedIndexRun {
                schema_version: 6,
                run_id: compact.run_id,
                delta: GraphDelta {
                    repository: generation.repository,
                    worktree: generation.worktree,
                    run_id: compact.run_id,
                    expected_base: generation.parent,
                    next_generation: generation.generation,
                    changed_files: Vec::new(),
                    removed_files: Vec::new(),
                    upsert_provenance: Vec::new(),
                    upsert_nodes: Vec::new(),
                    upsert_edges: Vec::new(),
                    remove_nodes: Vec::new(),
                    remove_edges: Vec::new(),
                },
                lineage: ChangeSetDelta::default(),
                consequences: ConsequenceDelta::default(),
                request_digest: Some(compact.request_digest),
                definition_digest: Some(compact.definition_digest),
                accepted_at: compact.accepted_at,
                events: compact.events,
                phase: compact.phase,
                accepted_generation: Some(compact.accepted_generation),
                rejection_reason: None,
            })
        }
        7 => {
            let compact: PersistedIndexRunV7 = bincode::deserialize(payload).map_err(|error| {
                WorkflowError::Verification(format!("decode compact Penelope v7 record: {error}"))
            })?;
            if compact.phase != RecordPhase::Rejected {
                return Err(WorkflowError::Verification(
                    "compact Penelope v7 record is not rejected".to_owned(),
                ));
            }
            Ok(PersistedIndexRun {
                schema_version: 7,
                run_id: compact.run_id,
                delta: GraphDelta {
                    repository: compact.repository,
                    worktree: compact.worktree,
                    run_id: compact.run_id,
                    expected_base: compact.expected_base,
                    next_generation: compact.next_generation,
                    changed_files: Vec::new(),
                    removed_files: Vec::new(),
                    upsert_provenance: Vec::new(),
                    upsert_nodes: Vec::new(),
                    upsert_edges: Vec::new(),
                    remove_nodes: Vec::new(),
                    remove_edges: Vec::new(),
                },
                lineage: ChangeSetDelta::default(),
                consequences: ConsequenceDelta::default(),
                request_digest: Some(compact.request_digest),
                definition_digest: Some(compact.definition_digest),
                accepted_at: compact.accepted_at,
                events: compact.events,
                phase: compact.phase,
                accepted_generation: None,
                rejection_reason: Some(compact.rejection_reason),
            })
        }
        other => Err(WorkflowError::Verification(format!(
            "unsupported Penelope index-record version {other}"
        ))),
    }
}

const fn receipt(run_id: IndexRunId, generation: GenerationManifest) -> WorkflowReceipt {
    let status = match generation.status {
        GenerationStatus::Durable => WorkflowStatus::Durable,
        GenerationStatus::VerificationPending => WorkflowStatus::VerificationPending,
        GenerationStatus::Verified => WorkflowStatus::Verified,
        GenerationStatus::VerificationFailed => WorkflowStatus::Failed,
    };
    WorkflowReceipt {
        run_id,
        generation,
        status,
    }
}

#[cfg(test)]
mod tests;
