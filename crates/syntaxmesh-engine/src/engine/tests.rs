use super::{Clock, EngineError, SyntaxMeshEngine, logical_integrity};
use syntaxmesh_core::{
    AcceptanceTime, EvidenceClass, FileId, FileVersion, GenerationId, GenerationManifest,
    GenerationStatus, GraphDelta, GraphDeltaWithLineage, IndexRunId, Node, NodeId, NodeKind,
    Provenance, ProvenanceId, RepositoryId, WorktreeId,
};
use syntaxmesh_integration_penelope::PenelopePublisher;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_language_sdk::SourceFile;
use syntaxmesh_store::{GraphStore, InMemoryGraphStore, StoreError};
use syntaxmesh_workflow::{DurableIndexWorkflow, WorkflowError, WorkflowStatus};

struct FixedClock(AcceptanceTime);

#[test]
fn consuming_engine_preserves_store_generation_and_workflow_records() -> Result<(), EngineError> {
    let repository = RepositoryId::derive(&[b"store-transfer-repository"]);
    let worktree = WorktreeId::derive(&[b"store-transfer-worktree"]);
    let generation = GenerationId::derive(&[b"store-transfer-generation"]);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    let content = "pub fn store_transfer() {}\n";
    let file = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"store-transfer-file"]),
            normalized_path: "lib.rs".to_owned(),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: content.len() as u64,
        },
        content: content.to_owned(),
    };
    engine.index(
        &[file],
        IndexRunId::derive(&[b"store-transfer-run"]),
        generation,
    )?;
    let diagnostics = engine.workflow_diagnostics()?;
    for enabled in [true, false] {
        engine.reconfigure_source_policies(None, enabled);
        if engine.verification_enabled != enabled || engine.workflow_diagnostics()? != diagnostics {
            return Err(EngineError::Workflow(WorkflowError::Verification(
                "source policy replacement changed workflow records".to_owned(),
            )));
        }
    }
    let store = engine.into_store();
    if store
        .current_generation(repository, worktree)?
        .is_none_or(|manifest| manifest.generation != generation)
    {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            "store transfer lost generation".to_owned(),
        )));
    }
    let mut reopened = SyntaxMeshEngine::new(store, RustExtractor, repository, worktree);
    if reopened.recover_pending_workflows()? != 0
        || reopened.workflow_diagnostics()?.completed_operations != 1
    {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            "store transfer lost completed workflow".to_owned(),
        )));
    }
    Ok(())
}

impl Clock for FixedClock {
    fn now(&self) -> Result<AcceptanceTime, String> {
        Ok(self.0)
    }
}

#[test]
fn publishes_and_verifies_one_generation() -> Result<(), EngineError> {
    let repository = RepositoryId::derive(&[b"engine-repo"]);
    let worktree = WorktreeId::derive(&[b"engine-worktree"]);
    let content = "fn main() {}\n";
    let file = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"src/main.rs"]),
            normalized_path: String::from("src/main.rs"),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: content.len() as u64,
        },
        content: String::from(content),
    };
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    )
    .with_clock(FixedClock(AcceptanceTime(8_123)))
    .with_statechronicle_verification();
    let generation = GenerationId::derive(&[b"engine-generation"]);
    let receipt = engine.index(&[file], IndexRunId::derive(&[b"engine-run"]), generation)?;
    let status = engine.status()?.ok_or_else(|| {
        EngineError::Workflow(WorkflowError::Verification(
            "engine status omitted its current generation".to_owned(),
        ))
    })?;
    let search_results = engine
        .query(receipt.publication.generation.generation)
        .search("main", 10)?;
    let Some(found) = search_results.first() else {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            "engine search omitted the indexed symbol".to_owned(),
        )));
    };
    let query_node_is_present = engine
        .query(receipt.publication.generation.generation)
        .node(found.id)?
        .is_some();
    if receipt.verification != WorkflowStatus::Verified
        || receipt.publication.generation.status != GenerationStatus::Durable
        || status.manifest.status != GenerationStatus::Verified
        || status.files != 1
        || status.nodes != 2
        || status.edges != 0
        || status.provenance != 1
        || !status.integrity.is_valid()
        || engine
            .current_generation(repository, worktree)?
            .is_none_or(|manifest| manifest.status != GenerationStatus::Verified)
        || !query_node_is_present
        || engine.indexer.store().acceptance_time(generation)? != Some(AcceptanceTime(8_123))
    {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            String::from("engine publication was not queryable and verified"),
        )));
    }
    Ok(())
}

#[test]
fn status_is_empty_before_first_publication() -> Result<(), EngineError> {
    let engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        RepositoryId::derive(&[b"empty-status-repo"]),
        WorktreeId::derive(&[b"empty-status-worktree"]),
    );
    if engine.status()?.is_some() {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            "empty engine reported a published generation".to_owned(),
        )));
    }
    Ok(())
}

#[test]
fn logical_integrity_reports_manifest_root_mismatch() -> Result<(), StoreError> {
    let manifest = GenerationManifest {
        repository: RepositoryId::derive(&[b"integrity-repository"]),
        worktree: WorktreeId::derive(&[b"integrity-worktree"]),
        generation: GenerationId::derive(&[b"integrity-generation"]),
        parent: None,
        graph_root: [0; 32],
        configuration_hash: [0; 32],
        extractor_set_hash: [0; 32],
        schema_version: 1,
        status: GenerationStatus::Durable,
    };
    let result = logical_integrity(&manifest, &[], &[], &[], &[])?;
    if result.graph_root_matches || !result.references_valid || result.is_valid() {
        return Err(StoreError::Integrity(
            "logical integrity failed to identify the graph-root mismatch".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn logical_integrity_reports_missing_fact_references() -> Result<(), StoreError> {
    let generation = GenerationId::derive(&[b"reference-integrity-generation"]);
    let node = Node {
        id: NodeId::derive(&[b"reference-integrity-node"]),
        kind: NodeKind::Function,
        name: "orphaned provenance".to_owned(),
        owner_file: None,
        source: None,
        provenance: ProvenanceId::derive(&[b"missing-provenance"]),
        extension_payload: None,
    };
    let node_bytes = serde_json::to_vec(&node).map_err(|error| {
        StoreError::Backend(format!("serialize test node for integrity check: {error}"))
    })?;
    let graph_root =
        *blake3::hash(&[generation.0.0.as_slice(), node_bytes.as_slice()].concat()).as_bytes();
    let manifest = GenerationManifest {
        repository: RepositoryId::derive(&[b"reference-integrity-repository"]),
        worktree: WorktreeId::derive(&[b"reference-integrity-worktree"]),
        generation,
        parent: None,
        graph_root,
        configuration_hash: [0; 32],
        extractor_set_hash: [0; 32],
        schema_version: 1,
        status: GenerationStatus::Durable,
    };
    let result = logical_integrity(&manifest, &[], &[node], &[], &[])?;
    if !result.graph_root_matches || result.references_valid || result.is_valid() {
        return Err(StoreError::Integrity(
            "logical integrity failed to identify missing fact provenance".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn index_freshness_counts_new_changed_and_removed_files() -> Result<(), EngineError> {
    let repository = RepositoryId::derive(&[b"freshness-repository"]);
    let worktree = WorktreeId::derive(&[b"freshness-worktree"]);
    let content = "fn indexed() {}\n";
    let indexed_file = FileVersion {
        file_id: FileId::derive(&[b"src/indexed.rs"]),
        normalized_path: "src/indexed.rs".to_owned(),
        content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
        size_bytes: content.len() as u64,
    };
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    engine.index(
        &[SourceFile {
            file: indexed_file.clone(),
            content: content.to_owned(),
        }],
        IndexRunId::derive(&[b"freshness-run"]),
        GenerationId::derive(&[b"freshness-generation"]),
    )?;
    let current = engine
        .index_freshness(std::slice::from_ref(&indexed_file))?
        .ok_or_else(|| {
            EngineError::Workflow(WorkflowError::Verification(
                "freshness check omitted the indexed generation".to_owned(),
            ))
        })?;
    let mut changed_file = indexed_file;
    changed_file.content_hash = *blake3::hash(b"fn indexed() { 2; }\n").as_bytes();
    let new_file = FileVersion {
        file_id: FileId::derive(&[b"src/new.rs"]),
        normalized_path: "src/new.rs".to_owned(),
        content_hash: *blake3::hash(b"fn added() {}\n").as_bytes(),
        size_bytes: 15,
    };
    let stale = engine
        .index_freshness(&[changed_file, new_file])?
        .ok_or_else(|| {
            EngineError::Workflow(WorkflowError::Verification(
                "freshness check omitted the indexed generation".to_owned(),
            ))
        })?;
    let removed = engine.index_freshness(&[])?.ok_or_else(|| {
        EngineError::Workflow(WorkflowError::Verification(
            "freshness check omitted the indexed generation".to_owned(),
        ))
    })?;
    if !current.is_current()
        || stale.is_current()
        || stale.unindexed_files != 1
        || stale.changed_files != 1
        || stale.removed_files != 0
        || removed.unindexed_files != 0
        || removed.changed_files != 0
        || removed.removed_files != 1
    {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            "source inventory differences were misclassified".to_owned(),
        )));
    }
    Ok(())
}

#[test]
fn verification_is_opt_in_and_preserves_durable_status() -> Result<(), EngineError> {
    let repository = RepositoryId::derive(&[b"unverified-repo"]);
    let worktree = WorktreeId::derive(&[b"unverified-worktree"]);
    let content = "fn main() {}\n";
    let file = SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"src/main.rs"]),
            normalized_path: String::from("src/main.rs"),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: content.len() as u64,
        },
        content: String::from(content),
    };
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    let receipt = engine.index(
        &[file],
        IndexRunId::derive(&[b"unverified-run"]),
        GenerationId::derive(&[b"unverified-generation"]),
    )?;
    if receipt.verification != WorkflowStatus::Durable
        || engine
            .current_generation(repository, worktree)?
            .is_none_or(|manifest| manifest.status != GenerationStatus::Durable)
    {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            String::from("disabled verification changed durable status"),
        )));
    }
    Ok(())
}

#[test]
fn prepared_explicit_lineage_uses_penelope_and_statechronicle_path() -> Result<(), EngineError> {
    let repository = RepositoryId::derive(&[b"lineage-engine-repo"]);
    let worktree = WorktreeId::derive(&[b"lineage-engine-worktree"]);
    let generation = GenerationId::derive(&[b"lineage-engine-generation"]);
    let run_id = IndexRunId::derive(&[b"lineage-engine-run"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"lineage-engine-provenance"]),
        producer_namespace: "test.change-set".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let graph = GraphDelta {
        repository,
        worktree,
        run_id,
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance.clone()],
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    let reference = InMemoryGraphStore::new();
    let event = reference.change_event_for_delta(&graph)?;
    let change_set = syntaxmesh_core::ChangeSetId::derive(&[b"lineage-engine-set"]);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    )
    .with_clock(FixedClock(AcceptanceTime(9_000)))
    .with_statechronicle_verification();
    let receipt = engine.publish_prepared_with_lineage(GraphDeltaWithLineage {
        graph,
        lineage: syntaxmesh_core::ChangeSetDelta {
            upsert_sets: vec![syntaxmesh_core::ChangeSet {
                id: change_set,
                kind: syntaxmesh_core::ChangeSetKind::ManualGroup,
                title: Some("engine publication".to_owned()),
                originating_intent: None,
                parent_changes: Vec::new(),
                git_commits: Vec::new(),
                pull_requests: Vec::new(),
                issues: Vec::new(),
                adrs: Vec::new(),
                repositories: vec![repository],
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
        },
    })?;
    let records = engine
        .query(generation)
        .events_for_change_set(change_set, None, 4)?;
    if receipt.verification != WorkflowStatus::Verified
        || engine.verify_statechronicle_history()?.is_none()
        || engine
            .current_generation(repository, worktree)?
            .is_none_or(|manifest| manifest.status != GenerationStatus::Verified)
        || !matches!(records.as_slice(), [
            syntaxmesh_api_model::TemporalRecord::ChangeSetEvent { item, .. },
            syntaxmesh_api_model::TemporalRecord::ChangeSetEventsFooter { returned: 1, has_more: false, .. }
        ] if item.event.id == event.id && item.membership.provenance == provenance.id)
    {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            "lineage publication bypassed Penelope or the enabled StateChronicle path".to_owned(),
        )));
    }
    Ok(())
}

#[test]
fn stale_prepared_parse_is_rejected_without_blocking_later_indexing() -> Result<(), EngineError> {
    let repository = RepositoryId::derive(&[b"race-repo"]);
    let worktree = WorktreeId::derive(&[b"race-worktree"]);
    let mut engine = SyntaxMeshEngine::new(
        InMemoryGraphStore::new(),
        RustExtractor,
        repository,
        worktree,
    );
    let file = |content: &str| SourceFile {
        file: FileVersion {
            file_id: FileId::derive(&[b"src/main.rs"]),
            normalized_path: String::from("src/main.rs"),
            content_hash: *blake3::hash(content.as_bytes()).as_bytes(),
            size_bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
        },
        content: String::from(content),
    };
    let baseline = engine.index(
        &[file("fn baseline() {}\n")],
        IndexRunId::derive(&[b"race-baseline-run"]),
        GenerationId::derive(&[b"race-baseline-generation"]),
    )?;
    let stale_delta = engine.indexer.prepare_delta(
        &[file("fn stale_result() {}\n")],
        IndexRunId::derive(&[b"race-stale-run"]),
        GenerationId::derive(&[b"race-stale-generation"]),
    )?;
    let winning_delta = engine.indexer.prepare_delta(
        &[file("fn latest_result() {}\n")],
        IndexRunId::derive(&[b"race-winning-run"]),
        GenerationId::derive(&[b"race-winning-generation"]),
    )?;
    let winning = {
        let mut publisher = PenelopePublisher::new(engine.indexer.store_mut());
        publisher.publish(winning_delta, AcceptanceTime(123))?
    };
    let stale_result = {
        let mut publisher = PenelopePublisher::new(engine.indexer.store_mut());
        publisher.publish(stale_delta, AcceptanceTime(456))
    };
    if !matches!(
        stale_result,
        Err(WorkflowError::Store(StoreError::StaleBase {
            expected: Some(expected),
            actual: Some(actual),
        })) if expected == baseline.publication.generation.generation
            && actual == winning.generation.generation
    ) {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            String::from("delayed parse was not rejected against the winning generation"),
        )));
    }
    let rejections = engine.workflow_rejections(None, 10)?;
    if rejections.items.len() != 1
        || rejections.items.first().is_none_or(|item| {
            item.run_id != IndexRunId::derive(&[b"race-stale-run"])
                || item.reason
                    != syntaxmesh_workflow::WorkflowRejectionReason::StaleBase {
                        repository,
                        worktree,
                        expected: Some(baseline.publication.generation.generation),
                        actual: Some(winning.generation.generation),
                    }
        })
        || rejections.next_cursor.is_some()
    {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            String::from("read-only engine diagnostics lost durable stale-base details"),
        )));
    }
    if engine.recover_pending_workflows()? != 0 {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            String::from("terminal stale workflow was recovered as pending"),
        )));
    }
    let latest_matches = engine
        .query(winning.generation.generation)
        .search("latest_result", 10)?;
    let stale_matches = engine
        .query(winning.generation.generation)
        .search("stale_result", 10)?;
    let later = engine.index(
        &[file("fn later_result() {}\n")],
        IndexRunId::derive(&[b"race-later-run"]),
        GenerationId::derive(&[b"race-later-generation"]),
    )?;
    if latest_matches.is_empty()
        || !stale_matches.is_empty()
        || later.publication.generation.parent != Some(winning.generation.generation)
    {
        return Err(EngineError::Workflow(WorkflowError::Verification(
            String::from("stale workflow changed the winner or blocked a later index"),
        )));
    }
    Ok(())
}
