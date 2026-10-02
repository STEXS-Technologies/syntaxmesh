use super::*;
use syntaxmesh_core::{GenerationId, GraphDelta, IndexRunId, RepositoryId, WorktreeId};
use syntaxmesh_store::{DurableRecordStore, FileGraphStore, InMemoryGraphStore};
use syntaxmesh_store_turso::TursoGraphStore;

#[test]
fn legacy_v1_record_decodes_with_unknown_acceptance_time() -> Result<(), WorkflowError> {
    let run_id = IndexRunId::derive(&[b"legacy-penelope-run"]);
    let legacy = PersistedIndexRunV1 {
        schema_version: 1,
        run_id,
        delta: test_delta(
            run_id,
            None,
            GenerationId::derive(&[b"legacy-penelope-generation"]),
        ),
        events: Vec::new(),
        phase: RecordPhase::Prepared,
        accepted_generation: None,
    };
    let payload = bincode::serialize(&legacy).map_err(|error| {
        WorkflowError::Verification(format!("encode legacy Penelope fixture: {error}"))
    })?;
    let decoded = decode_record(&payload)?;
    if decoded.schema_version != 1 || decoded.run_id != run_id || decoded.accepted_at.is_some() {
        return Err(WorkflowError::Verification(
            "legacy Penelope record did not retain unknown acceptance time".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn legacy_v2_record_decodes_with_empty_explicit_lineage() -> Result<(), WorkflowError> {
    let run_id = IndexRunId::derive(&[b"legacy-penelope-v2-run"]);
    let accepted_at = AcceptanceTime(456);
    let legacy = PersistedIndexRunV2 {
        schema_version: 2,
        run_id,
        delta: test_delta(
            run_id,
            None,
            GenerationId::derive(&[b"legacy-penelope-v2-generation"]),
        ),
        accepted_at: Some(accepted_at),
        events: Vec::new(),
        phase: RecordPhase::Prepared,
        accepted_generation: None,
    };
    let payload = bincode::serialize(&legacy).map_err(|error| {
        WorkflowError::Verification(format!("encode legacy Penelope v2 fixture: {error}"))
    })?;
    let decoded = decode_record(&payload)?;
    if decoded.schema_version != 2
        || decoded.run_id != run_id
        || decoded.accepted_at != Some(accepted_at)
        || !decoded.lineage.is_empty()
    {
        return Err(WorkflowError::Verification(
            "legacy Penelope v2 record did not retain its historical semantics".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn legacy_v3_record_keeps_lineage_and_empty_consequences() -> Result<(), WorkflowError> {
    let run_id = IndexRunId::derive(&[b"legacy-penelope-v3-run"]);
    let accepted_at = AcceptanceTime(789);
    let legacy = PersistedIndexRunV3 {
        schema_version: 3,
        run_id,
        delta: test_delta(
            run_id,
            None,
            GenerationId::derive(&[b"legacy-v3-generation"]),
        ),
        lineage: ChangeSetDelta::default(),
        accepted_at: Some(accepted_at),
        events: Vec::new(),
        phase: RecordPhase::Prepared,
        accepted_generation: None,
    };
    let payload = bincode::serialize(&legacy).map_err(|error| {
        WorkflowError::Verification(format!("encode legacy Penelope v3 fixture: {error}"))
    })?;
    let decoded = decode_record(&payload)?;
    if decoded.schema_version != 3
        || decoded.run_id != run_id
        || decoded.accepted_at != Some(accepted_at)
        || !decoded.consequences.is_empty()
    {
        return Err(WorkflowError::Verification(
            "legacy Penelope v3 record did not preserve its historical shape".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn penelope_journals_explicit_lineage_and_replays_it_after_restart() -> Result<(), WorkflowError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-penelope-{}-lineage.snapshot",
        std::process::id()
    ));
    let run_id = IndexRunId::derive(&[b"penelope-lineage-run"]);
    let generation = GenerationId::derive(&[b"penelope-lineage-generation"]);
    let provenance = syntaxmesh_core::Provenance {
        id: syntaxmesh_core::ProvenanceId::derive(&[b"penelope-lineage-provenance"]),
        producer_namespace: "test.penelope-lineage".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    };
    let mut delta = test_delta(run_id, None, generation);
    delta.upsert_provenance.push(provenance.clone());
    let change_set = syntaxmesh_core::ChangeSetId::derive(&[b"penelope-lineage-set"]);
    let event_store = InMemoryGraphStore::new();
    let event = event_store.change_event_for_delta(&delta)?;
    let lineage = ChangeSetDelta {
        upsert_sets: vec![syntaxmesh_core::ChangeSet {
            id: change_set,
            kind: syntaxmesh_core::ChangeSetKind::ManualGroup,
            title: Some("recorded through Penelope".to_owned()),
            originating_intent: None,
            parent_changes: Vec::new(),
            git_commits: Vec::new(),
            pull_requests: Vec::new(),
            issues: Vec::new(),
            adrs: Vec::new(),
            repositories: vec![delta.repository],
            first_generation: generation,
            last_generation: Some(generation),
            provenance: provenance.id,
        }],
        assign_events: vec![syntaxmesh_core::ChangeSetMembership {
            change_set,
            event: event.id,
            provenance: provenance.id,
        }],
        unassign_events: Vec::new(),
    };
    let request = GraphDeltaWithLineage {
        graph: delta,
        lineage: lineage.clone(),
    };
    let mut workflow =
        PenelopeWorkflow::new(FileGraphStore::open(&path).map_err(WorkflowError::Store)?);
    workflow.publish_with_lineage(request.clone(), AcceptanceTime(321))?;
    let mut conflicting_retry = request;
    if let Some(set) = conflicting_retry.lineage.upsert_sets.first_mut() {
        set.title = Some("different retry evidence".to_owned());
    }
    if !matches!(
        workflow.publish_with_lineage(conflicting_retry, AcceptanceTime(321)),
        Err(WorkflowError::Verification(_))
    ) {
        return Err(WorkflowError::Verification(
            "Penelope accepted a retry with changed lineage evidence".to_owned(),
        ));
    }
    let store = workflow.into_store();
    drop(store);
    let reopened = FileGraphStore::open(&path).map_err(WorkflowError::Store)?;
    let stored = reopened
        .read_record(&record_key(run_id))?
        .ok_or_else(|| WorkflowError::Verification("completed record disappeared".to_owned()))?;
    if decode_record(&stored)?.schema_version != 6 {
        return Err(WorkflowError::Verification(
            "completed Penelope record was not stored in compact form".to_owned(),
        ));
    }
    let entry = reopened
        .generation_lineage_history()?
        .into_iter()
        .find(|entry| entry.generation == generation);
    let page = reopened.events_for_change_set(change_set, generation, None, 2)?;
    if entry.as_ref().map(|entry| &entry.delta) != Some(&lineage)
        || page.items.first().map(|item| item.event.id) != Some(event.id)
    {
        return Err(WorkflowError::Verification(
            "Penelope recovery lost the accepted ChangeSet lineage".to_owned(),
        ));
    }
    drop(reopened);
    std::fs::remove_file(path).map_err(|error| {
        WorkflowError::Verification(format!("remove Penelope lineage fixture: {error}"))
    })?;
    Ok(())
}

#[test]
fn plans_with_penelope_before_publication() -> Result<(), WorkflowError> {
    let mut workflow = PenelopeWorkflow::new(InMemoryGraphStore::new());
    let run_id = IndexRunId::derive(&[b"penelope-run"]);
    let delta = test_delta(run_id, None, GenerationId::derive(&[b"generation"]));
    let receipt = workflow.publish(delta.clone(), AcceptanceTime(123))?;
    if receipt.run_id != run_id
        || workflow
            .last_decision()
            .is_none_or(|decision| decision.projection.status != SagaStatus::Completed)
    {
        return Err(WorkflowError::Verification(
            "Penelope decision was not retained".to_owned(),
        ));
    }
    let mut store = workflow.into_store();
    if store.acceptance_time(receipt.generation.generation)? != Some(AcceptanceTime(123)) {
        return Err(WorkflowError::Verification(
            "published generation lost its engine acceptance time".to_owned(),
        ));
    }
    store.set_generation_status(receipt.generation.generation, GenerationStatus::Verified)?;
    let mut verified_workflow = PenelopeWorkflow::new(store);
    let mut repeated_delta = delta;
    repeated_delta.expected_base = Some(receipt.generation.generation);
    let repeated = verified_workflow.publish(repeated_delta, AcceptanceTime(456))?;
    if repeated.status != WorkflowStatus::Verified
        || repeated.generation.status != GenerationStatus::Verified
    {
        return Err(WorkflowError::Verification(
            "idempotent Penelope receipt hid persisted verification status".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn completed_record_compacts_request_but_keeps_digest_bound_replay() -> Result<(), WorkflowError> {
    let run_id = IndexRunId::derive(&[b"compact-penelope-run"]);
    let generation = GenerationId::derive(&[b"compact-penelope-generation"]);
    let mut delta = test_delta(run_id, None, generation);
    delta.changed_files.push(syntaxmesh_core::FileVersion {
        file_id: syntaxmesh_core::FileId::derive(&[b"large-source-file"]),
        normalized_path: format!("src/{}", "x".repeat(256 * 1024)),
        content_hash: [7; 32],
        size_bytes: 256 * 1024,
    });
    let prepared = prepare_record(
        delta.clone(),
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(123),
    )?;
    let uncompressed_size = encode_record(&prepared)?.len();
    let manifest = GenerationManifest {
        repository: delta.repository,
        worktree: delta.worktree,
        generation,
        parent: None,
        graph_root: [1; 32],
        configuration_hash: [2; 32],
        extractor_set_hash: [3; 32],
        schema_version: 1,
        status: GenerationStatus::Durable,
    };
    let (completed, _) = complete_record(prepared, manifest)?;
    let compact_payload = encode_record(&completed)?;
    let compact = decode_record(&compact_payload)?;
    if compact_payload.len() * 100 >= uncompressed_size
        || compact.schema_version != 6
        || replay_record(&compact)?.projection.status != SagaStatus::Completed
        || !record_matches_request(
            &compact,
            &delta,
            &ChangeSetDelta::default(),
            &ConsequenceDelta::default(),
        )?
    {
        return Err(WorkflowError::Verification(
            "completed Penelope record did not compact while retaining digest replay".to_owned(),
        ));
    }
    let mut different_request = delta;
    let Some(changed_file) = different_request.changed_files.first_mut() else {
        return Err(WorkflowError::Verification(
            "compact-request fixture lost its file".to_owned(),
        ));
    };
    changed_file.content_hash = [8; 32];
    if record_matches_request(
        &compact,
        &different_request,
        &ChangeSetDelta::default(),
        &ConsequenceDelta::default(),
    )? {
        return Err(WorkflowError::Verification(
            "compact Penelope record accepted a different request digest".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn consequence_assertions_are_bound_to_penelope_publication() -> Result<(), WorkflowError> {
    let store = InMemoryGraphStore::new();
    let run_id = IndexRunId::derive(&[b"penelope-consequence-run"]);
    let generation = GenerationId::derive(&[b"penelope-consequence-generation"]);
    let provenance = syntaxmesh_core::Provenance {
        id: syntaxmesh_core::ProvenanceId::derive(&[b"penelope-consequence-provenance"]),
        producer_namespace: "test.penelope-consequence".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: syntaxmesh_core::EvidenceClass::SourceFact,
        source: None,
    };
    let node_id = syntaxmesh_core::NodeId::derive(&[b"penelope-consequence-node"]);
    let mut graph = test_delta(run_id, None, generation);
    graph.upsert_provenance.push(provenance.clone());
    graph.upsert_nodes.push(syntaxmesh_core::Node {
        id: node_id,
        kind: syntaxmesh_core::NodeKind::Function,
        name: "asserted-symbol".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    });
    let event = store.change_event_for_delta(&graph)?;
    let fact = syntaxmesh_core::FactVersionRef {
        fact: syntaxmesh_core::FactRef::Node(node_id),
        valid_from: generation,
    };
    let request = GraphDeltaWithConsequences {
        publication: GraphDeltaWithLineage {
            graph,
            lineage: ChangeSetDelta::default(),
        },
        consequences: ConsequenceDelta {
            add: vec![syntaxmesh_core::ConsequenceEdge {
                id: syntaxmesh_core::ConsequenceEdgeId::derive(&[b"penelope-consequence-edge"]),
                source: syntaxmesh_core::LineageEndpoint::ChangeEvent(event.id),
                target: syntaxmesh_core::LineageEndpoint::FactVersion(fact),
                kind: syntaxmesh_core::ConsequenceKind::DirectDependencyEffect,
                evidence: vec![fact],
                derivation: syntaxmesh_core::ConsequenceDerivation::Explicit,
                provenance: provenance.id,
            }],
            retract: Vec::new(),
        },
    };
    let mut workflow = PenelopeWorkflow::new(store);
    let receipt = workflow.publish_with_consequences(request.clone(), AcceptanceTime(987))?;
    let persisted = workflow.into_store().generation_consequence_history()?;
    if persisted
        .iter()
        .find(|entry| entry.generation == receipt.generation.generation)
        .map(|entry| &entry.delta)
        != Some(&request.consequences)
    {
        return Err(WorkflowError::Verification(
            "Penelope publication lost explicit consequences".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn diagnostics_report_prepared_and_completed_runs_without_recovery_side_effects()
-> Result<(), WorkflowError> {
    let mut store = InMemoryGraphStore::new();
    let run_id = IndexRunId::derive(&[b"diagnostics-run"]);
    let delta = test_delta(
        run_id,
        None,
        GenerationId::derive(&[b"diagnostics-generation"]),
    );
    let prepared = prepare_record(
        delta,
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(123),
    )?;
    store.compare_exchange_record(&record_key(run_id), None, &encode_record(&prepared)?)?;

    let before = PenelopePublisher::<InMemoryGraphStore>::inspect(&store)?;
    if before.prepared_operations != 1
        || before.completed_operations != 0
        || before.rejected_operations != 0
    {
        return Err(WorkflowError::Verification(
            "diagnostics did not report one prepared run".to_owned(),
        ));
    }
    let mut workflow = PenelopeWorkflow::new(store);
    if workflow.recover_pending()? != 1 {
        return Err(WorkflowError::Verification(
            "prepared diagnostic fixture was not recovered".to_owned(),
        ));
    }
    let completed_store = workflow.into_store();
    let after = PenelopePublisher::<InMemoryGraphStore>::inspect(&completed_store)?;
    if after.prepared_operations != 0 || after.completed_operations != 1 {
        return Err(WorkflowError::Verification(
            "diagnostics did not reflect the completed recovery".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn rejection_diagnostics_survive_restart_are_paged_and_do_not_recover() -> Result<(), WorkflowError>
{
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-penelope-{}-rejection-diagnostics.snapshot",
        std::process::id()
    ));
    let first_run = IndexRunId::derive(&[b"rejection-page-a"]);
    let second_run = IndexRunId::derive(&[b"rejection-page-b"]);
    let (first, second) = if first_run.0.to_hex() < second_run.0.to_hex() {
        (first_run, second_run)
    } else {
        (second_run, first_run)
    };
    let expected = Some(GenerationId::derive(&[b"rejection-expected"]));
    let actual = Some(GenerationId::derive(&[b"rejection-actual"]));
    let mut store = FileGraphStore::open(&path).map_err(WorkflowError::Store)?;

    for run_id in [first, second] {
        let delta = test_delta(
            run_id,
            expected,
            GenerationId::derive(&[run_id.0.to_hex().as_bytes()]),
        );
        let prepared = prepare_record(
            delta,
            ChangeSetDelta::default(),
            ConsequenceDelta::default(),
            AcceptanceTime(123),
        )?;
        let key = record_key(run_id);
        let encoded = encode_record(&prepared)?;
        store.compare_exchange_record(&key, None, &encoded)?;
        reject_prepared_record(
            &mut store,
            &key,
            &encoded,
            prepared,
            PersistedRejectionReason::StaleBase { expected, actual },
        )?;
    }

    let pending_id = IndexRunId::derive(&[b"rejection-inspection-pending"]);
    let pending = prepare_record(
        test_delta(
            pending_id,
            None,
            GenerationId::derive(&[b"rejection-inspection-pending-generation"]),
        ),
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(456),
    )?;
    let pending_key = record_key(pending_id);
    let pending_bytes = encode_record(&pending)?;
    store.compare_exchange_record(&pending_key, None, &pending_bytes)?;
    drop(store);

    let reopened_store = FileGraphStore::open(&path).map_err(WorkflowError::Store)?;
    let first_page =
        PenelopePublisher::<FileGraphStore>::inspect_rejections(&reopened_store, None, 1)?;
    if first_page.items.len() != 1 || first_page.next_cursor != Some(first) {
        return Err(WorkflowError::Verification(
            "rejection diagnostics did not return a deterministic first page".to_owned(),
        ));
    }
    let second_page = PenelopePublisher::<FileGraphStore>::inspect_rejections(
        &reopened_store,
        first_page.next_cursor,
        1,
    )?;
    let Some(second_item) = second_page.items.first().copied() else {
        return Err(WorkflowError::Verification(
            "rejection diagnostics omitted the second page item".to_owned(),
        ));
    };
    let scope = test_delta(
        second,
        expected,
        GenerationId::derive(&[second.0.to_hex().as_bytes()]),
    );
    if second_page.items.len() != 1
        || second_item.run_id != second
        || second_page.next_cursor.is_some()
        || !matches!(
            second_item.reason,
            WorkflowRejectionReason::StaleBase {
                repository,
                worktree,
                expected: actual_expected,
                actual: actual_generation,
            } if repository == scope.repository
                && worktree == scope.worktree
                && actual_expected == expected
                && actual_generation == actual
        )
    {
        return Err(WorkflowError::Verification(
            "rejection diagnostics lost typed stale-base context or cursor order".to_owned(),
        ));
    }
    if reopened_store.read_record(&pending_key)?.as_deref() != Some(pending_bytes.as_slice()) {
        return Err(WorkflowError::Verification(
            "read-only rejection diagnostics mutated a prepared workflow".to_owned(),
        ));
    }
    let compacted = reopened_store
        .read_record(&record_key(first))?
        .ok_or_else(|| WorkflowError::Verification("rejected record disappeared".to_owned()))?;
    let decoded_rejection = decode_record(&compacted)?;
    if decoded_rejection.schema_version != 7
        || !decoded_rejection.delta.upsert_nodes.is_empty()
        || !decoded_rejection.delta.upsert_edges.is_empty()
        || !decoded_rejection.delta.changed_files.is_empty()
        || decoded_rejection.rejection_reason.is_none()
    {
        return Err(WorkflowError::Verification(
            "rejected workflow was not compacted into the v7 typed record".to_owned(),
        ));
    }
    if replay_record(&decoded_rejection)?.projection.status != SagaStatus::Escalated {
        return Err(WorkflowError::Verification(
            "compacted rejected workflow no longer replays its terminal saga decision".to_owned(),
        ));
    }

    let mut retry_store = reopened_store;
    let retry_delta = test_delta(
        first,
        expected,
        GenerationId::derive(&[first.0.to_hex().as_bytes()]),
    );
    if !matches!(
        publish_index(&mut retry_store, retry_delta, AcceptanceTime(123)),
        Err(WorkflowError::Store(StoreError::StaleBase {
            expected: found_expected,
            actual: found_actual,
        })) if found_expected == expected && found_actual == actual
    ) {
        return Err(WorkflowError::Verification(
            "retry of compact rejected operation did not preserve stale-base error".to_owned(),
        ));
    }
    drop(retry_store);
    std::fs::remove_file(path).map_err(|error| {
        WorkflowError::Verification(format!("remove rejection fixture: {error}"))
    })?;
    Ok(())
}

#[test]
fn legacy_rejected_records_are_reported_as_unknown() -> Result<(), WorkflowError> {
    let run_id = IndexRunId::derive(&[b"legacy-rejection-reason"]);
    let delta = test_delta(
        run_id,
        None,
        GenerationId::derive(&[b"legacy-rejection-generation"]),
    );
    let prepared = prepare_record(
        delta,
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(123),
    )?;
    let (mut legacy, _) = reject_record(
        prepared,
        PersistedRejectionReason::StaleBase {
            expected: None,
            actual: None,
        },
    )?;
    legacy.schema_version = 4;
    legacy.request_digest = None;
    legacy.definition_digest = None;
    legacy.rejection_reason = None;

    let mut store = InMemoryGraphStore::new();
    store.compare_exchange_record(&record_key(run_id), None, &encode_record(&legacy)?)?;
    let page = PenelopePublisher::<InMemoryGraphStore>::inspect_rejections(&store, None, 10)?;
    if page.items.len() != 1
        || page.items.first().is_none_or(|item| {
            item.run_id != run_id || item.reason != WorkflowRejectionReason::UnknownLegacy
        })
    {
        return Err(WorkflowError::Verification(
            "legacy rejection diagnostics guessed a reason or omitted the record".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn exact_retry_compacts_an_existing_v4_completed_record() -> Result<(), WorkflowError> {
    let run_id = IndexRunId::derive(&[b"compact-v4-retry-run"]);
    let generation = GenerationId::derive(&[b"compact-v4-retry-generation"]);
    let delta = test_delta(run_id, None, generation);
    let mut store = InMemoryGraphStore::new();
    let accepted = store
        .apply_delta(delta.clone())
        .map_err(WorkflowError::Store)?;
    let prepared = prepare_record(
        delta.clone(),
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(123),
    )?;
    let (mut legacy_completed, _) = complete_record(prepared, accepted)?;
    legacy_completed.schema_version = 4;
    legacy_completed.request_digest = None;
    legacy_completed.definition_digest = None;
    let key = record_key(run_id);
    store.compare_exchange_record(&key, None, &encode_record(&legacy_completed)?)?;

    let mut workflow = PenelopeWorkflow::new(store);
    workflow.publish(delta, AcceptanceTime(123))?;
    let completed_store = workflow.into_store();
    let compacted = decode_record(&completed_store.read_record(&key)?.ok_or_else(|| {
        WorkflowError::Verification("completed v4 record disappeared".to_owned())
    })?)?;
    if compacted.schema_version != 6
        || compacted.request_digest.is_none()
        || compacted.definition_digest.is_none()
    {
        return Err(WorkflowError::Verification(
            "exact retry did not compact the completed v4 Penelope record".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn exact_retry_compacts_v3_while_preserving_legacy_definition_digest() -> Result<(), WorkflowError>
{
    let run_id = IndexRunId::derive(&[b"compact-v3-retry-run"]);
    let generation = GenerationId::derive(&[b"compact-v3-retry-generation"]);
    let delta = test_delta(run_id, None, generation);
    let lineage = ChangeSetDelta::default();
    let consequences = ConsequenceDelta::default();
    let legacy_context = saga_context(&delta, &lineage, &consequences, false)?;
    let started = start(
        &legacy_context.definition,
        legacy_context.tenant_id.clone(),
        legacy_context.process_id.clone(),
        legacy_context.action_id.clone(),
    )
    .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    let start_event = LinearSagaInput::Start {
        input_id: InputId::new(format!("inp_start_{}", run_id.0.to_hex()))
            .map_err(|error| WorkflowError::Verification(error.to_string()))?,
        action_id: legacy_context.action_id,
    }
    .to_event(&legacy_context.definition);
    let prepared = PersistedIndexRun {
        schema_version: 3,
        run_id,
        delta: delta.clone(),
        lineage,
        consequences,
        request_digest: None,
        definition_digest: None,
        accepted_at: Some(AcceptanceTime(123)),
        events: vec![LinearSagaEventEnvelope {
            sequence: 0,
            event: start_event,
        }],
        phase: RecordPhase::Prepared,
        accepted_generation: None,
        rejection_reason: None,
    };
    if replay_record(&prepared)? != started {
        return Err(WorkflowError::Verification(
            "legacy v3 fixture does not replay its original definition".to_owned(),
        ));
    }

    let mut store = InMemoryGraphStore::new();
    let accepted = store
        .apply_delta(delta.clone())
        .map_err(WorkflowError::Store)?;
    let (mut legacy_completed, _) = complete_record(prepared, accepted)?;
    legacy_completed.schema_version = 3;
    legacy_completed.request_digest = None;
    legacy_completed.definition_digest = None;
    let key = record_key(run_id);
    store.compare_exchange_record(&key, None, &encode_record(&legacy_completed)?)?;

    let mut workflow = PenelopeWorkflow::new(store);
    workflow.publish(delta, AcceptanceTime(123))?;
    let completed_store = workflow.into_store();
    let compacted = decode_record(&completed_store.read_record(&key)?.ok_or_else(|| {
        WorkflowError::Verification("completed v3 record disappeared".to_owned())
    })?)?;
    if compacted.schema_version != 6
        || compacted.request_digest == compacted.definition_digest
        || replay_record(&compacted)?.projection.status != SagaStatus::Completed
    {
        return Err(WorkflowError::Verification(
            "v3 compaction did not preserve separate request and definition digests".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn recovers_prepared_publication_before_and_after_graph_commit() -> Result<(), WorkflowError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-penelope-{}-recovery.snapshot",
        std::process::id()
    ));
    let first_generation = GenerationId::derive(&[b"recovery-first-generation"]);
    let first_delta = test_delta(
        IndexRunId::derive(&[b"recovery-first-run"]),
        None,
        first_generation,
    );
    let first_record = prepare_record(
        first_delta.clone(),
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(123),
    )?;
    let first_key = record_key(first_delta.run_id);
    let mut first_store = FileGraphStore::open(&path).map_err(WorkflowError::Store)?;
    first_store.compare_exchange_record(&first_key, None, &encode_record(&first_record)?)?;
    first_store
        .apply_delta_with_acceptance_time(first_delta, first_record.accepted_at)
        .map_err(WorkflowError::Store)?;
    drop(first_store);

    let second_generation = GenerationId::derive(&[b"recovery-second-generation"]);
    let second_delta = test_delta(
        IndexRunId::derive(&[b"recovery-second-run"]),
        Some(first_generation),
        second_generation,
    );
    let second_record = prepare_record(
        second_delta.clone(),
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(456),
    )?;
    let second_key = record_key(second_delta.run_id);
    let mut staged_store = FileGraphStore::open(&path).map_err(WorkflowError::Store)?;
    staged_store.compare_exchange_record(&second_key, None, &encode_record(&second_record)?)?;
    drop(staged_store);

    let restarted_store = FileGraphStore::open(&path).map_err(WorkflowError::Store)?;
    let mut workflow = PenelopeWorkflow::new(restarted_store);
    let recovered = workflow.recover_pending()?;
    let recovered_again = workflow.recover_pending()?;
    let completed_store = workflow.into_store();
    let latest = completed_store
        .latest_generation()
        .ok_or_else(|| WorkflowError::Verification("recovery lost graph manifest".to_owned()))?;
    let first =
        decode_record(&completed_store.read_record(&first_key)?.ok_or_else(|| {
            WorkflowError::Verification("first process record missing".to_owned())
        })?)?;
    let second =
        decode_record(&completed_store.read_record(&second_key)?.ok_or_else(|| {
            WorkflowError::Verification("second process record missing".to_owned())
        })?)?;
    let first_acceptance = completed_store.acceptance_time(first_generation)?;
    let second_acceptance = completed_store.acceptance_time(second_generation)?;
    std::mem::drop(std::fs::remove_file(path));
    if recovered != 2
        || recovered_again != 0
        || latest.generation != second_generation
        || first.phase != RecordPhase::Completed
        || second.phase != RecordPhase::Completed
        || replay_record(&first)?.projection.status != SagaStatus::Completed
        || replay_record(&second)?.projection.status != SagaStatus::Completed
        || first_acceptance != Some(AcceptanceTime(123))
        || second_acceptance != Some(AcceptanceTime(456))
    {
        return Err(WorkflowError::Verification(
            "Penelope recovery did not complete both interrupted index runs".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn turso_journal_recovers_interrupted_index_operations() -> Result<(), WorkflowError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-penelope-{}-turso-recovery.db",
        std::process::id()
    ));
    let first_generation = GenerationId::derive(&[b"turso-recovery-first-generation"]);
    let first_delta = test_delta(
        IndexRunId::derive(&[b"turso-recovery-first-run"]),
        None,
        first_generation,
    );
    let first_record = prepare_record(
        first_delta.clone(),
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(123),
    )?;
    let first_key = record_key(first_delta.run_id);
    TursoGraphStore::migrate(&path)
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    let mut first_store = TursoGraphStore::open(&path)
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    first_store.compare_exchange_record(&first_key, None, &encode_record(&first_record)?)?;
    first_store
        .apply_delta_with_acceptance_time(first_delta, first_record.accepted_at)
        .map_err(WorkflowError::Store)?;
    drop(first_store);

    let second_generation = GenerationId::derive(&[b"turso-recovery-second-generation"]);
    let second_delta = test_delta(
        IndexRunId::derive(&[b"turso-recovery-second-run"]),
        Some(first_generation),
        second_generation,
    );
    let second_record = prepare_record(
        second_delta.clone(),
        ChangeSetDelta::default(),
        ConsequenceDelta::default(),
        AcceptanceTime(456),
    )?;
    let second_key = record_key(second_delta.run_id);
    let mut staged_store = TursoGraphStore::open(&path)
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    staged_store.compare_exchange_record(&second_key, None, &encode_record(&second_record)?)?;
    drop(staged_store);

    let restarted_store = TursoGraphStore::open(&path)
        .map_err(|error| WorkflowError::Verification(error.to_string()))?;
    let mut workflow = PenelopeWorkflow::new(restarted_store);
    let recovered = workflow.recover_pending()?;
    let recovered_again = workflow.recover_pending()?;
    let completed_store = workflow.into_store();
    let latest = completed_store.latest_generation().ok_or_else(|| {
        WorkflowError::Verification("Turso recovery lost graph manifest".to_owned())
    })?;
    let first =
        decode_record(&completed_store.read_record(&first_key)?.ok_or_else(|| {
            WorkflowError::Verification("Turso first record missing".to_owned())
        })?)?;
    let second =
        decode_record(&completed_store.read_record(&second_key)?.ok_or_else(|| {
            WorkflowError::Verification("Turso second record missing".to_owned())
        })?)?;
    let first_acceptance = completed_store.acceptance_time(first_generation)?;
    let second_acceptance = completed_store.acceptance_time(second_generation)?;
    drop(completed_store);
    std::mem::drop(std::fs::remove_file(path));
    if recovered != 2
        || recovered_again != 0
        || latest.generation != second_generation
        || first.phase != RecordPhase::Completed
        || second.phase != RecordPhase::Completed
        || replay_record(&first)?.projection.status != SagaStatus::Completed
        || replay_record(&second)?.projection.status != SagaStatus::Completed
        || first_acceptance != Some(AcceptanceTime(123))
        || second_acceptance != Some(AcceptanceTime(456))
    {
        return Err(WorkflowError::Verification(
            "Turso Penelope recovery did not complete interrupted index runs".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn test_delta(
    run_id: IndexRunId,
    expected_base: Option<GenerationId>,
    next_generation: GenerationId,
) -> GraphDelta {
    GraphDelta {
        repository: RepositoryId::derive(&[b"recovery-repo"]),
        worktree: WorktreeId::derive(&[b"recovery-worktree"]),
        run_id,
        expected_base,
        next_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    }
}
