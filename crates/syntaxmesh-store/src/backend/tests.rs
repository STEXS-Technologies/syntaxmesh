use super::*;
use syntaxmesh_core::{Edge, EvidenceClass, GenerationStatus, NodeKind, RelationKind};

#[test]
fn temporal_page_constructors_are_feature_independent() {
    let edges = HistoricalEdgePage::new(Vec::new(), false);
    assert!(edges.items.is_empty());
    assert!(!edges.has_more);
    let consequences = ConsequenceRangePage::new(Vec::new(), None);
    assert!(consequences.items.is_empty());
    assert!(consequences.next_cursor.is_none());
    #[cfg(feature = "benchmark-instrumentation")]
    {
        assert_eq!(edges.read_metrics, HistoricalEdgeReadMetrics::default());
        assert_eq!(
            consequences.read_metrics,
            ConsequenceRangeReadMetrics::default()
        );
        let measured_edges = edges.with_read_metrics(HistoricalEdgeReadMetrics {
            sql_read_statements: 7,
            ..HistoricalEdgeReadMetrics::default()
        });
        let measured_consequences = consequences.with_read_metrics(ConsequenceRangeReadMetrics {
            sql_read_statements: 11,
            ..ConsequenceRangeReadMetrics::default()
        });
        assert_eq!(measured_edges.read_metrics.sql_read_statements, 7);
        assert_eq!(measured_consequences.read_metrics.sql_read_statements, 11);
    }
}
#[test]
fn class_node_kind_is_appended_after_existing_bincode_variants() {
    let previous_last = bincode::serialize(&NodeKind::RuntimeObservation).unwrap_or_default();
    let class = bincode::serialize(&NodeKind::Class).unwrap_or_default();
    let decoded_previous =
        bincode::deserialize::<NodeKind>(&[12, 0, 0, 0]).unwrap_or(NodeKind::Class);
    assert_eq!(previous_last, [12, 0, 0, 0]);
    assert_eq!(class, [13, 0, 0, 0]);
    assert_eq!(decoded_previous, NodeKind::RuntimeObservation);
}

fn delta(expected_base: Option<GenerationId>) -> GraphDelta {
    let repository = RepositoryId::derive(&[b"repo"]);
    let worktree = WorktreeId::derive(&[b"worktree"]);
    let generation = GenerationId::derive(&[b"generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"test"]),
        producer_namespace: "test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let provenance_id = provenance.id;
    let node_id = NodeId::derive(&[b"node"]);
    let target_id = NodeId::derive(&[b"target"]);
    let node = Node {
        id: node_id,
        kind: NodeKind::Function,
        name: "main".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance_id,
        extension_payload: None,
    };
    let target = Node {
        id: target_id,
        kind: NodeKind::Function,
        name: "target".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance_id,
        extension_payload: None,
    };
    GraphDelta {
        repository,
        worktree,
        run_id: syntaxmesh_core::IndexRunId::derive(&[b"run"]),
        expected_base,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![node, target],
        upsert_edges: vec![Edge {
            id: EdgeId::derive(&[b"edge"]),
            source: node_id,
            target: target_id,
            relation: RelationKind::Calls,
            provenance: provenance_id,
            extension_payload: None,
        }],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    }
}

fn consequence_request(
    graph: GraphDelta,
    evidence_generation: GenerationId,
) -> Result<GraphDeltaWithConsequences, StoreError> {
    let generation = graph.next_generation;
    let provenance = graph
        .upsert_provenance
        .first()
        .map(|item| item.id)
        .ok_or_else(|| StoreError::InvalidDelta("sample graph has no provenance".to_owned()))?;
    let event = InMemoryGraphStore::new().change_event_for_delta(&graph)?;
    let edge_id = ConsequenceEdgeId::derive(&[b"consequence"]);
    Ok(GraphDeltaWithConsequences {
        publication: GraphDeltaWithLineage {
            graph,
            lineage: ChangeSetDelta::default(),
        },
        consequences: ConsequenceDelta {
            add: vec![ConsequenceEdge {
                id: edge_id,
                source: LineageEndpoint::ChangeEvent(event.id),
                target: LineageEndpoint::FactVersion(FactVersionRef {
                    fact: FactRef::Node(NodeId::derive(&[b"node"])),
                    valid_from: generation,
                }),
                kind: syntaxmesh_core::ConsequenceKind::DirectDependencyEffect,
                evidence: vec![FactVersionRef {
                    fact: FactRef::Edge(EdgeId::derive(&[b"edge"])),
                    valid_from: evidence_generation,
                }],
                derivation: syntaxmesh_core::ConsequenceDerivation::Explicit,
                provenance,
            }],
            retract: Vec::new(),
        },
    })
}

#[test]
fn reference_consequence_range_preserves_retraction_bounds_and_cursor_binding()
-> Result<(), StoreError> {
    let mut store = InMemoryGraphStore::new();
    let first = delta(None);
    let first_generation = first.next_generation;
    let event = store.change_event_for_delta(&first)?;
    let first_request = consequence_request(first, first_generation)?;
    let (edge_id, provenance) = first_request
        .consequences
        .add
        .first()
        .map(|edge| (edge.id, edge.provenance))
        .ok_or_else(|| StoreError::Integrity("consequence fixture has no edge".to_owned()))?;
    store.apply_delta_with_consequences(first_request, None)?;

    let mut second = delta(Some(first_generation));
    second.next_generation = GenerationId::derive(&[b"range-retract-generation"]);
    let second_generation = second.next_generation;
    store.apply_delta_with_consequences(
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: second,
                lineage: ChangeSetDelta::default(),
            },
            consequences: ConsequenceDelta {
                add: Vec::new(),
                retract: vec![syntaxmesh_core::ConsequenceRetraction {
                    edge: edge_id,
                    provenance,
                }],
            },
        },
        None,
    )?;

    let endpoint = LineageEndpoint::ChangeEvent(event.id);
    let page = store.consequence_edges_for_endpoint_range(
        endpoint,
        first_generation,
        second_generation,
        None,
        1,
    )?;
    let item = page.items.first().ok_or_else(|| {
        StoreError::Integrity("reference range query omitted a retracted edge".to_owned())
    })?;
    if item.edge.id != edge_id
        || item.valid_from != first_generation
        || item.valid_until != Some(second_generation)
        || page.next_cursor.is_some()
    {
        return Err(StoreError::Integrity(
            "reference consequence range returned an incorrect validity interval".to_owned(),
        ));
    }
    if !store
        .consequence_edges_for_endpoint_range(
            endpoint,
            second_generation,
            second_generation,
            None,
            1,
        )?
        .items
        .is_empty()
    {
        return Err(StoreError::Integrity(
            "exclusive retraction generation still included the consequence edge".to_owned(),
        ));
    }
    if store
        .consequence_edges_for_endpoint_range(
            endpoint,
            second_generation,
            first_generation,
            None,
            1,
        )
        .is_ok()
    {
        return Err(StoreError::Integrity(
            "reversed consequence range was accepted".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn in_memory_generation_root_matches_full_snapshot_oracle() -> Result<(), StoreError> {
    let mut memory = InMemoryGraphStore::new();
    let delta = delta(None);
    let generation = delta.next_generation;
    let manifest = memory.apply_delta(delta)?;
    let snapshot = GraphSnapshot {
        files: memory.files.values().cloned().collect(),
        provenance: memory.provenance.values().cloned().collect(),
        nodes: memory.nodes.values().cloned().collect(),
        edges: memory.edges.values().cloned().collect(),
    };
    let oracle_root = graph_snapshot_root_v2(generation, &snapshot)?;
    if manifest.graph_root != oracle_root {
        return Err(StoreError::Integrity(
            "in-memory streaming root differs from the full-snapshot oracle".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn in_memory_incremental_root_matches_full_snapshot_after_cascade() -> Result<(), StoreError> {
    let mut memory = InMemoryGraphStore::new();
    let first = delta(None);
    let first_manifest = memory.apply_delta(first)?;
    let mut update = delta(Some(first_manifest.generation));
    update.next_generation = GenerationId::derive(&[b"generation-2"]);
    update.upsert_provenance.clear();
    update.upsert_nodes.clear();
    update.upsert_edges.clear();
    update.remove_nodes.push(NodeId::derive(&[b"target"]));

    let manifest = memory.apply_delta(update)?;
    let snapshot = GraphSnapshot {
        files: memory.files.values().cloned().collect(),
        provenance: memory.provenance.values().cloned().collect(),
        nodes: memory.nodes.values().cloned().collect(),
        edges: memory.edges.values().cloned().collect(),
    };
    let oracle_root = graph_snapshot_root_v2(manifest.generation, &snapshot)?;
    let cached_root = generation_root_v2(manifest.generation.0, memory.persistent_fact_root);
    if manifest.graph_root != oracle_root
        || !memory.persistent_fact_root_ready
        || manifest.graph_root != cached_root
    {
        return Err(StoreError::Integrity(
            "incremental in-memory fact root differs from full-snapshot oracle".to_owned(),
        ));
    }
    let anchor = GenerationHistoryEntry {
        manifest,
        delta: None,
        anchor: Some(snapshot),
    };
    let mut restored = InMemoryGraphStore::new();
    restored.restore_history_anchor(&anchor)?;
    if !restored.persistent_fact_root_ready
        || restored.persistent_fact_root != memory.persistent_fact_root
    {
        return Err(StoreError::Integrity(
            "restored snapshot did not retain its canonical fact-tree cache".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn explicit_consequence_publication_is_atomic_and_file_restartable() -> Result<(), StoreError> {
    let mut memory = InMemoryGraphStore::new();
    let graph = delta(None);
    let generation = graph.next_generation;
    let request = consequence_request(graph, generation)?;
    let edge_id = request
        .consequences
        .add
        .first()
        .map(|edge| edge.id)
        .ok_or_else(|| StoreError::Integrity("test edge was not constructed".to_owned()))?;
    let endpoint = request
        .consequences
        .add
        .first()
        .map(|edge| edge.source)
        .ok_or_else(|| StoreError::Integrity("test consequence has no endpoint".to_owned()))?;
    let accepted = memory.apply_delta_with_consequences(request, None)?;
    let memory_page = memory.consequence_edges_for_endpoint(endpoint, generation, None, 10)?;
    let history = memory.generation_consequence_history()?;
    if history.len() != 1
        || memory_page.items.len() != 1
        || memory_page.items.first().map(|edge| edge.id) != Some(edge_id)
        || history
            .first()
            .and_then(|entry| entry.delta.add.first())
            .map(|edge| edge.id)
            != Some(edge_id)
    {
        return Err(StoreError::Integrity(
            "in-memory consequence journal did not retain the accepted edge".to_owned(),
        ));
    }

    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-store-{}-consequence-restart.snapshot",
        std::process::id()
    ));
    let mut file = FileGraphStore::open(&path)?;
    let file_graph = delta(None);
    let file_generation = file_graph.next_generation;
    file.apply_delta_with_consequences(consequence_request(file_graph, file_generation)?, None)?;
    drop(file);
    let reopened = FileGraphStore::open(&path)?;
    let restored = reopened.generation_consequence_history()?;
    let file_page = reopened.consequence_edges_for_endpoint(endpoint, file_generation, None, 10)?;
    let integrity = reopened.backend_integrity_check()?;
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove consequence fixture: {error}")))?;
    if restored.len() != 1
        || file_page.items.len() != 1
        || file_page.items.first().map(|edge| edge.id) != Some(edge_id)
        || restored
            .first()
            .and_then(|entry| entry.delta.add.first())
            .map(|edge| edge.id)
            != Some(edge_id)
        || accepted.generation != generation
        || !integrity.passed
    {
        return Err(StoreError::Integrity(
            "FileGraphStore consequence publication did not survive restart".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn empty_consequence_delta_accepts_next_generation_without_rebuilding_history()
-> Result<(), StoreError> {
    let mut memory = InMemoryGraphStore::new();
    let graph = delta(None);
    let first_generation = graph.next_generation;
    let accepted = memory
        .apply_delta_with_consequences(consequence_request(graph, first_generation)?, None)?;

    let mut next_graph = delta(Some(accepted.generation));
    next_graph.next_generation = GenerationId::derive(&[b"generation-2"]);
    next_graph.upsert_provenance.clear();
    next_graph.upsert_nodes.clear();
    next_graph.upsert_edges.clear();
    let next_generation = next_graph.next_generation;
    let next = memory.apply_delta_with_consequences(
        GraphDeltaWithConsequences {
            publication: GraphDeltaWithLineage {
                graph: next_graph,
                lineage: ChangeSetDelta::default(),
            },
            consequences: ConsequenceDelta::default(),
        },
        None,
    )?;

    let history = memory.generation_consequence_history()?;
    if next.generation != next_generation
        || history.len() != 2
        || !history.get(1).is_some_and(|entry| entry.delta.is_empty())
    {
        return Err(StoreError::Integrity(
            "empty consequence delta did not preserve accepted history".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn invalid_consequence_evidence_does_not_publish_graph_generation() -> Result<(), StoreError> {
    let mut store = InMemoryGraphStore::new();
    let graph = delta(None);
    let request = consequence_request(graph, GenerationId::derive(&[b"unknown"]))?;
    if store.apply_delta_with_consequences(request, None).is_ok()
        || !store.generation_history()?.is_empty()
        || !store.generation_consequence_history()?.is_empty()
    {
        return Err(StoreError::Integrity(
            "invalid consequence evidence partially published a generation".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn version_one_snapshot_migrates_empty_consequence_history() -> Result<(), StoreError> {
    let mut memory = InMemoryGraphStore::new();
    let manifest = memory.apply_delta(delta(None))?;
    let legacy = FileSnapshotV1 {
        version: 1,
        memory: InMemoryGraphStoreV1 {
            repository: memory.repository,
            worktree: memory.worktree,
            manifest: memory.manifest,
            history: memory.history.iter().cloned().collect(),
            files: memory
                .files
                .iter()
                .map(|(key, value)| (*key, value.clone()))
                .collect(),
            provenance: memory
                .provenance
                .iter()
                .map(|(key, value)| (*key, value.clone()))
                .collect(),
            nodes: memory
                .nodes
                .iter()
                .map(|(key, value)| (*key, value.clone()))
                .collect(),
            edges: memory
                .edges
                .iter()
                .map(|(key, value)| (*key, value.clone()))
                .collect(),
            records: memory
                .records
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            lineage_history: memory.lineage_history,
        },
    };
    let mut bytes = FILE_SNAPSHOT_MAGIC.to_vec();
    bytes.extend(bincode::serialize(&legacy).map_err(|error| {
        StoreError::Backend(format!("encode v1 snapshot test fixture: {error}"))
    })?);
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-store-{}-v1-snapshot.snapshot",
        std::process::id()
    ));
    std::fs::write(&path, bytes)
        .map_err(|error| StoreError::Backend(format!("write v1 snapshot fixture: {error}")))?;
    let reopened = FileGraphStore::open(&path)?;
    let history = reopened.generation_consequence_history()?;
    let current = reopened.current_generation(manifest.repository, manifest.worktree)?;
    drop(reopened);
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove v1 snapshot fixture: {error}")))?;
    if current != Some(manifest)
        || history.len() != 1
        || !history.first().is_some_and(|entry| entry.delta.is_empty())
    {
        return Err(StoreError::Integrity(
            "v1 FileGraphStore snapshot did not migrate with empty consequence history".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn publishes_atomically_and_reads_one_generation() -> Result<(), StoreError> {
    let mut store = InMemoryGraphStore::new();
    let accepted_delta = delta(None);
    let manifest = store.apply_delta(accepted_delta.clone())?;
    if manifest.schema_version != 2 {
        return Err(StoreError::Integrity(
            "new generation did not use root schema version two".to_owned(),
        ));
    }
    let history = store.generation_history()?;
    if manifest.status != GenerationStatus::Durable {
        return Err(StoreError::Integrity(
            "new generation is not durable".to_owned(),
        ));
    }
    let node_id = NodeId::derive(&[b"node"]);
    if !store
        .node(manifest.generation, node_id)
        .is_ok_and(|node| node.is_some())
    {
        return Err(StoreError::Integrity(
            "published node cannot be read".to_owned(),
        ));
    }
    let history_matches = history.first().is_some_and(|entry| {
        history.len() == 1
            && entry.manifest == manifest
            && entry.delta.as_ref() == Some(&accepted_delta)
    });
    if !history_matches {
        return Err(StoreError::Integrity(
            "accepted generation delta was not retained in history".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn delta_reference_failure_does_not_publish_candidate_state() -> Result<(), StoreError> {
    let mut store = InMemoryGraphStore::new();
    let first = delta(None);
    let first_generation = first.next_generation;
    store.apply_delta(first)?;

    let mut invalid = delta(Some(first_generation));
    invalid.next_generation = GenerationId::derive(&[b"invalid-reference-generation"]);
    invalid.upsert_provenance.clear();
    let missing = ProvenanceId::derive(&[b"missing-delta-provenance"]);
    let Some(node) = invalid.upsert_nodes.first_mut() else {
        return Err(StoreError::Integrity(
            "reference test fixture has no node".to_owned(),
        ));
    };
    node.provenance = missing;

    let result = store.apply_delta(invalid);
    if !matches!(result, Err(StoreError::Integrity(_)))
        || store.current_manifest().map(|manifest| manifest.generation) != Some(first_generation)
    {
        return Err(StoreError::Integrity(
            "missing changed-fact provenance published an invalid generation".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn rejects_stale_base_without_mutating_state() -> Result<(), StoreError> {
    let mut store = InMemoryGraphStore::new();
    let first = store.apply_delta(delta(None))?;
    let rejected = store.apply_delta(delta(None));
    if !matches!(rejected, Err(StoreError::StaleBase { .. })) {
        return Err(StoreError::Integrity("stale delta was accepted".to_owned()));
    }
    if store
        .current_generation(first.repository, first.worktree)
        .ok()
        .flatten()
        != Some(first)
    {
        return Err(StoreError::Integrity(
            "stale delta mutated state".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn file_store_survives_restart() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-store-{}-{}.json",
        std::process::id(),
        "restart"
    ));
    let mut store = FileGraphStore::open(&path)?;
    let manifest = store.apply_delta(delta(None))?;
    let accepted_history = store.generation_history()?;
    store.compare_exchange_record("test.operation/run", None, b"prepared")?;
    let manifest = store.set_generation_status(manifest.generation, GenerationStatus::Verified)?;
    drop(store);
    let reopened = FileGraphStore::open(&path)?;
    let current = reopened.current_generation(manifest.repository, manifest.worktree)?;
    let record = reopened.read_record("test.operation/run")?;
    let outgoing = reopened.outgoing(manifest.generation, NodeId::derive(&[b"node"]))?;
    let incoming = reopened.incoming(manifest.generation, NodeId::derive(&[b"target"]))?;
    let restored_history = reopened.generation_history()?;
    std::mem::drop(std::fs::remove_file(path));
    if current != Some(manifest)
        || record.as_deref() != Some(b"prepared")
        || outgoing.len() != 1
        || incoming.len() != 1
        || outgoing.first().map(|edge| edge.id) != incoming.first().map(|edge| edge.id)
        || restored_history != accepted_history
    {
        return Err(StoreError::Integrity(
            "persisted generation was not restored".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn file_store_reads_legacy_bincode_and_integrity_understands_envelope() -> Result<(), StoreError> {
    let path = std::env::temp_dir().join(format!(
        "syntaxmesh-store-{}-legacy.snapshot",
        std::process::id(),
    ));
    let legacy = LegacyFileSnapshot {
        repository: None,
        worktree: None,
        manifest: None,
        history: Vec::new(),
        files: BTreeMap::new(),
        provenance: BTreeMap::new(),
        nodes: BTreeMap::new(),
        edges: BTreeMap::new(),
        records: BTreeMap::new(),
    };
    std::fs::write(
        &path,
        bincode::serialize(&legacy).map_err(|error| {
            StoreError::Backend(format!("encode legacy test snapshot: {error}"))
        })?,
    )
    .map_err(|error| StoreError::Backend(format!("write legacy test snapshot: {error}")))?;
    let legacy_store = FileGraphStore::open(&path)?;
    if !legacy_store.backend_integrity_check()?.passed {
        return Err(StoreError::Integrity(
            "legacy snapshot failed integrity after open".to_owned(),
        ));
    }
    drop(legacy_store);
    let mut store = FileGraphStore::open(&path)?;
    store.apply_delta(delta(None))?;
    drop(store);
    let bytes = std::fs::read(&path)
        .map_err(|error| StoreError::Backend(format!("read migrated test snapshot: {error}")))?;
    if !bytes.starts_with(FILE_SNAPSHOT_MAGIC) {
        return Err(StoreError::Integrity(
            "legacy snapshot was not rewritten with the versioned envelope".to_owned(),
        ));
    }
    std::fs::remove_file(path)
        .map_err(|error| StoreError::Backend(format!("remove legacy test snapshot: {error}")))?;
    Ok(())
}

#[test]
fn ordered_persistent_maps_preserve_btree_bincode_layout() -> Result<(), StoreError> {
    let legacy = BTreeMap::from([
        (2_u64, String::from("second")),
        (1_u64, String::from("first")),
    ]);
    let legacy_bytes = bincode::serialize(&legacy)
        .map_err(|error| StoreError::Backend(format!("encode legacy map: {error}")))?;
    let persistent = bincode::deserialize::<OrdMap<u64, String>>(&legacy_bytes)
        .map_err(|error| StoreError::Backend(format!("decode persistent map: {error}")))?;
    let persistent_bytes = bincode::serialize(&persistent)
        .map_err(|error| StoreError::Backend(format!("encode persistent map: {error}")))?;
    let restored = bincode::deserialize::<BTreeMap<u64, String>>(&persistent_bytes)
        .map_err(|error| StoreError::Backend(format!("decode legacy map: {error}")))?;
    if persistent_bytes != legacy_bytes || restored != legacy {
        return Err(StoreError::Integrity(
            "persistent ordered-map encoding changed the BTreeMap snapshot layout".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn persistent_history_preserves_vec_bincode_layout() -> Result<(), StoreError> {
    let legacy = vec![String::from("first"), String::from("second")];
    let legacy_bytes = bincode::serialize(&legacy)
        .map_err(|error| StoreError::Backend(format!("encode legacy history: {error}")))?;
    let persistent = bincode::deserialize::<Vector<String>>(&legacy_bytes)
        .map_err(|error| StoreError::Backend(format!("decode persistent history: {error}")))?;
    let persistent_bytes = bincode::serialize(&persistent)
        .map_err(|error| StoreError::Backend(format!("encode persistent history: {error}")))?;
    let restored = bincode::deserialize::<Vec<String>>(&persistent_bytes)
        .map_err(|error| StoreError::Backend(format!("decode legacy history: {error}")))?;
    if persistent_bytes != legacy_bytes || restored != legacy {
        return Err(StoreError::Integrity(
            "persistent history encoding changed the Vec snapshot layout".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn failed_file_snapshot_replacement_keeps_published_state_unchanged() -> Result<(), StoreError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0_u128, |duration| duration.as_nanos());
    let directory = std::env::temp_dir().join(format!(
        "syntaxmesh-store-failed-publish-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory)
        .map_err(|error| StoreError::Backend(format!("create test directory: {error}")))?;
    let snapshot_path = directory.join("snapshot");
    std::fs::create_dir(&snapshot_path)
        .map_err(|error| StoreError::Backend(format!("create blocking directory: {error}")))?;
    let mut store = FileGraphStore {
        path: snapshot_path,
        memory: InMemoryGraphStore::new(),
    };

    let result = store.apply_delta(delta(None));
    let state_unchanged = store.latest_generation().is_none();
    let temporary_files = std::fs::read_dir(&directory)
        .map_err(|error| StoreError::Backend(format!("read test directory: {error}")))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
        .count();
    std::fs::remove_dir_all(&directory)
        .map_err(|error| StoreError::Backend(format!("remove test directory: {error}")))?;

    if result.is_ok() || !state_unchanged || temporary_files != 0 {
        return Err(StoreError::Integrity(
            "failed snapshot replacement changed state or leaked a temporary file".to_owned(),
        ));
    }
    Ok(())
}
