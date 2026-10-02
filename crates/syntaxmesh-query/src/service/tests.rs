use super::*;
use syntaxmesh_core::{
    ConsequenceDelta, ConsequenceDerivation, ConsequenceEdge, ConsequenceEdgeId, ConsequenceKind,
    Edge, EdgeId, EvidenceClass, FactVersionRef, GenerationId, GraphDelta,
    GraphDeltaWithConsequences, GraphDeltaWithLineage, IndexRunId, NodeKind, Provenance,
    ProvenanceId, RelationKind, RepositoryId, WorktreeId,
};
use syntaxmesh_store::InMemoryGraphStore;

mod fact_history_pages;

#[test]
fn consequence_trace_preserves_delayed_chronology_and_exclusive_retractions()
-> Result<(), QueryError> {
    let repository = RepositoryId::derive(&[b"trace-repo"]);
    let worktree = WorktreeId::derive(&[b"trace-worktree"]);
    let generations = [
        GenerationId::derive(&[b"trace-g1"]),
        GenerationId::derive(&[b"trace-g2"]),
        GenerationId::derive(&[b"trace-g3"]),
        GenerationId::derive(&[b"trace-g4"]),
    ];
    let [g1, g2, g3, g4] = generations;
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"trace-provenance"]),
        producer_namespace: "test.consequence-trace".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let nodes = ["origin", "middle", "delayed", "late", "expired"]
        .into_iter()
        .map(|name| Node {
            id: NodeId::derive(&[name.as_bytes()]),
            kind: NodeKind::Function,
            name: name.to_owned(),
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        })
        .collect::<Vec<_>>();
    let node_id = |index: usize| {
        nodes
            .get(index)
            .map(|node| node.id)
            .ok_or(QueryError::InvalidLimit)
    };
    let edge = |name: &[u8], source: usize, target: usize| -> Result<ConsequenceEdge, QueryError> {
        let evidence = FactVersionRef {
            fact: FactRef::Node(node_id(source)?),
            valid_from: g1,
        };
        Ok(ConsequenceEdge {
            id: ConsequenceEdgeId::derive(&[name]),
            source: LineageEndpoint::FactVersion(FactVersionRef {
                fact: FactRef::Node(node_id(source)?),
                valid_from: g1,
            }),
            target: LineageEndpoint::FactVersion(FactVersionRef {
                fact: FactRef::Node(node_id(target)?),
                valid_from: g1,
            }),
            kind: ConsequenceKind::DirectDependencyEffect,
            evidence: vec![evidence],
            derivation: ConsequenceDerivation::Explicit,
            provenance: provenance.id,
        })
    };
    let ab = edge(b"trace-ab", 0, 1)?;
    let ab_late = edge(b"trace-ab-late", 0, 1)?;
    let bc = edge(b"trace-bc", 1, 2)?;
    let de = edge(b"trace-de", 3, 4)?;
    let ad = edge(b"trace-ad", 0, 3)?;
    let mut store = InMemoryGraphStore::new();
    let apply = |target_store: &mut InMemoryGraphStore,
                 expected,
                 generation,
                 run,
                 upsert_nodes,
                 upsert_provenance,
                 add,
                 retract| {
        target_store.apply_delta_with_consequences(
            GraphDeltaWithConsequences {
                publication: GraphDeltaWithLineage {
                    graph: GraphDelta {
                        repository,
                        worktree,
                        run_id: IndexRunId::derive(&[run]),
                        expected_base: expected,
                        next_generation: generation,
                        changed_files: Vec::new(),
                        removed_files: Vec::new(),
                        upsert_provenance,
                        upsert_nodes,
                        upsert_edges: Vec::new(),
                        remove_nodes: Vec::new(),
                        remove_edges: Vec::new(),
                    },
                    lineage: Default::default(),
                },
                consequences: ConsequenceDelta { add, retract },
            },
            None,
        )?;
        Ok::<_, QueryError>(())
    };
    apply(
        &mut store,
        None,
        g1,
        b"trace-run-1",
        nodes.clone(),
        vec![provenance.clone()],
        vec![ab.clone()],
        Vec::new(),
    )?;
    apply(
        &mut store,
        Some(g1),
        g2,
        b"trace-run-2",
        Vec::new(),
        Vec::new(),
        vec![de.clone()],
        vec![syntaxmesh_core::ConsequenceRetraction {
            edge: ab.id,
            provenance: provenance.id,
        }],
    )?;
    apply(
        &mut store,
        Some(g2),
        g3,
        b"trace-run-3",
        Vec::new(),
        Vec::new(),
        vec![bc.clone(), ab_late.clone()],
        Vec::new(),
    )?;
    apply(
        &mut store,
        Some(g3),
        g4,
        b"trace-run-4",
        Vec::new(),
        Vec::new(),
        vec![ad],
        vec![syntaxmesh_core::ConsequenceRetraction {
            edge: de.id,
            provenance: provenance.id,
        }],
    )?;

    let origin = LineageEndpoint::FactVersion(FactVersionRef {
        fact: FactRef::Node(node_id(0)?),
        valid_from: g1,
    });
    let delayed_id = node_id(2)?;
    let middle_id = node_id(1)?;
    let expired_id = node_id(4)?;
    let query = Query::new(&store, g4);
    let request = ConsequenceTraceRequest {
        origin,
        from_generation: g1,
        until_generation: g4,
        max_hops: 8,
        max_endpoints: 20,
        max_edges: 20,
        max_scanned_incidences: 100,
        included_kinds: Vec::new(),
        included_evidence_classes: vec![EvidenceClass::UserAsserted],
    };
    let trace = query.consequence_trace(&request)?;
    #[cfg(feature = "benchmark-instrumentation")]
    if trace.read_metrics.generation_sequence_lookups > generations.len() {
        return Err(QueryError::InvalidLimit);
    }
    let reached_delayed = trace.states.iter().any(|item| {
        item.endpoint
            == LineageEndpoint::FactVersion(FactVersionRef {
                fact: FactRef::Node(delayed_id),
                valid_from: g1,
            })
            && item.reached_generation == g3
    });
    let reached_expired = trace.states.iter().any(|item| {
        item.endpoint
            == LineageEndpoint::FactVersion(FactVersionRef {
                fact: FactRef::Node(expired_id),
                valid_from: g1,
            })
    });
    let preserved_later_alternate = trace.states.iter().any(|item| {
        item.endpoint
            == LineageEndpoint::FactVersion(FactVersionRef {
                fact: FactRef::Node(middle_id),
                valid_from: g1,
            })
            && item.reached_generation == g3
    }) && trace
        .hops
        .iter()
        .any(|hop| hop.version.edge.id == ab_late.id);
    if !reached_delayed
        || !preserved_later_alternate
        || reached_expired
        || !trace
            .hops
            .iter()
            .any(|hop| hop.version.edge.id == ab.id && hop.version.valid_until == Some(g2))
        || !trace
            .hops
            .iter()
            .any(|hop| hop.version.edge.id == bc.id && hop.reached_generation == g3)
        || trace.hops.iter().any(|hop| hop.version.edge.id == de.id)
        || trace.truncated
    {
        return Err(QueryError::InvalidLimit);
    }
    let excluded = query.consequence_trace(&ConsequenceTraceRequest {
        included_evidence_classes: vec![EvidenceClass::SourceFact],
        ..request.clone()
    })?;
    let class_union = query.consequence_trace(&ConsequenceTraceRequest {
        included_evidence_classes: vec![EvidenceClass::SourceFact, EvidenceClass::UserAsserted],
        ..request.clone()
    })?;
    if !excluded.hops.is_empty()
        || excluded.states.len() != 1
        || class_union.hops.len() != trace.hops.len()
        || class_union.states != trace.states
    {
        return Err(QueryError::InvalidLimit);
    }
    let records =
        query.consequence_trace_records(&request, MAX_HISTORICAL_NEIGHBORHOOD_RESULT_BYTES)?;
    if !matches!(records.first(), Some(TemporalRecord::ConsequenceTraceHeader {
        origin: record_origin, from_generation, until_generation,
        schema_version: 1, ..
    }) if *record_origin == origin && *from_generation == g1 && *until_generation == g4)
        || !matches!(records.last(), Some(TemporalRecord::ConsequenceTraceFooter {
            states, hops, truncated: false, ..
        }) if *states == u64::try_from(trace.states.len()).unwrap_or(u64::MAX)
            && *hops == u64::try_from(trace.hops.len()).unwrap_or(u64::MAX))
    {
        return Err(QueryError::InvalidLimit);
    }
    let cap_header = TemporalRecord::ConsequenceTraceHeader {
        schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
        query_mode: TemporalQueryMode::HistoricalConclusion,
        origin,
        from_generation: g1,
        until_generation: g4,
    };
    let cap_footer = TemporalRecord::ConsequenceTraceFooter {
        schema_version: CONSEQUENCE_TRACE_SCHEMA_VERSION,
        query_mode: TemporalQueryMode::HistoricalConclusion,
        states: u64::MAX,
        hops: u64::MAX,
        scanned_incidences: u64::MAX,
        truncated: true,
    };
    let framing_cap = serialized_line_size(&cap_header, MAX_HISTORICAL_NEIGHBORHOOD_RESULT_BYTES)?
        .unwrap_or(0)
        .saturating_add(
            serialized_line_size(&cap_footer, MAX_HISTORICAL_NEIGHBORHOOD_RESULT_BYTES)?
                .unwrap_or(0),
        )
        .saturating_add(1);
    let capped_records = query.consequence_trace_records(&request, framing_cap)?;
    let capped_bytes = capped_records
        .iter()
        .filter_map(|record| record.to_json_line().ok())
        .map(|line| line.len().saturating_add(1))
        .sum::<usize>();
    if capped_bytes > framing_cap
        || !matches!(
            capped_records.last(),
            Some(TemporalRecord::ConsequenceTraceFooter {
                truncated: true,
                ..
            })
        )
    {
        return Err(QueryError::InvalidLimit);
    }
    Ok(())
}

#[test]
fn graph_at_known_by_uses_ancestry_maximum_and_reports_unknown_prefix() -> Result<(), QueryError> {
    let repository = RepositoryId::derive(&[b"known-by-repo"]);
    let worktree = WorktreeId::derive(&[b"known-by-worktree"]);
    let first = GenerationId::derive(&[b"known-by-first"]);
    let second = GenerationId::derive(&[b"known-by-second"]);
    let third = GenerationId::derive(&[b"known-by-third"]);
    let mut store = InMemoryGraphStore::new();
    let mut parent = None;
    for (generation, accepted_at) in [(first, 10), (second, 30), (third, 20)] {
        store.apply_delta_at(
            GraphDelta {
                repository,
                worktree,
                run_id: IndexRunId::derive(&[generation.0.0.as_slice()]),
                expected_base: parent,
                next_generation: generation,
                changed_files: Vec::new(),
                removed_files: Vec::new(),
                upsert_provenance: Vec::new(),
                upsert_nodes: Vec::new(),
                upsert_edges: Vec::new(),
                remove_nodes: Vec::new(),
                remove_edges: Vec::new(),
            },
            AcceptanceTime(accepted_at),
        )?;
        parent = Some(generation);
    }
    let query = Query::new(&store, third);
    if !matches!(
        query.export_graph_at_known_by(third, AcceptanceTime(29)),
        Err(QueryError::GenerationNotKnownBy {
            accepted_through: AcceptanceTime(30),
            ..
        })
    ) {
        return Err(QueryError::Store(StoreError::Integrity(
            "known-by cutoff ignored a later ancestor acceptance".to_owned(),
        )));
    }
    let records = query.export_graph_at_known_by(third, AcceptanceTime(30))?;
    if !matches!(records.first(), Some(GraphRecord::Header {
        generation, accepted_by: Some(AcceptanceTime(30)),
        accepted_through: Some(AcceptanceTime(30)), ..
    }) if *generation == third)
    {
        return Err(QueryError::Store(StoreError::Integrity(
            "known-by export header omitted its temporal qualification".to_owned(),
        )));
    }
    let mut unknown_store = InMemoryGraphStore::new();
    let unknown_parent = GenerationId::derive(&[b"known-by-unknown-parent"]);
    let unknown_ancestor = GenerationId::derive(&[b"known-by-unknown-ancestor"]);
    let unknown_target = GenerationId::derive(&[b"known-by-unknown-target"]);
    let make_delta = |base: Option<GenerationId>, next: GenerationId| GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[next.0.0.as_slice()]),
        expected_base: base,
        next_generation: next,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    unknown_store.apply_delta_at(make_delta(None, unknown_parent), AcceptanceTime(10))?;
    unknown_store.apply_delta(make_delta(Some(unknown_parent), unknown_ancestor))?;
    unknown_store.apply_delta_at(
        make_delta(Some(unknown_ancestor), unknown_target),
        AcceptanceTime(20),
    )?;
    if !matches!(
        Query::new(&unknown_store, unknown_target)
            .export_graph_at_known_by(unknown_target, AcceptanceTime(20)),
        Err(QueryError::UnknownAcceptanceHistory(generation)) if generation == unknown_target
    ) {
        return Err(QueryError::Store(StoreError::Integrity(
            "unknown ancestor acceptance time was not propagated".to_owned(),
        )));
    }
    Ok(())
}

#[test]
fn consequence_neighborhood_is_bounded_weakly_connected_and_deterministic() -> Result<(), QueryError>
{
    let repository = RepositoryId::derive(&[b"consequence-query-repo"]);
    let worktree = WorktreeId::derive(&[b"consequence-query-worktree"]);
    let generation = GenerationId::derive(&[b"consequence-query-generation"]);
    let run_id = IndexRunId::derive(&[b"consequence-query-run"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"consequence-query-provenance"]),
        producer_namespace: "test.consequence-query".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let first = Node {
        id: NodeId::derive(&[b"consequence-query-first"]),
        kind: NodeKind::Function,
        name: "first".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let second = Node {
        id: NodeId::derive(&[b"consequence-query-second"]),
        kind: NodeKind::Function,
        name: "second".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let first_ref = FactVersionRef {
        fact: FactRef::Node(first.id),
        valid_from: generation,
    };
    let second_ref = FactVersionRef {
        fact: FactRef::Node(second.id),
        valid_from: generation,
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
        upsert_nodes: vec![first, second],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    let mut store = InMemoryGraphStore::new();
    let event = store.change_event_for_delta(&graph)?;
    store.apply_delta_with_consequences(
        syntaxmesh_core::GraphDeltaWithConsequences {
            publication: syntaxmesh_core::GraphDeltaWithLineage {
                graph,
                lineage: syntaxmesh_core::ChangeSetDelta::default(),
            },
            consequences: syntaxmesh_core::ConsequenceDelta {
                add: vec![
                    ConsequenceEdge {
                        id: syntaxmesh_core::ConsequenceEdgeId::derive(&[b"consequence-query-a"]),
                        source: LineageEndpoint::ChangeEvent(event.id),
                        target: LineageEndpoint::FactVersion(first_ref),
                        kind: syntaxmesh_core::ConsequenceKind::DirectDependencyEffect,
                        evidence: vec![first_ref],
                        derivation: syntaxmesh_core::ConsequenceDerivation::Explicit,
                        provenance: provenance.id,
                    },
                    ConsequenceEdge {
                        id: syntaxmesh_core::ConsequenceEdgeId::derive(&[b"consequence-query-b"]),
                        source: LineageEndpoint::FactVersion(first_ref),
                        target: LineageEndpoint::FactVersion(second_ref),
                        kind: syntaxmesh_core::ConsequenceKind::TestEffect,
                        evidence: vec![first_ref],
                        derivation: syntaxmesh_core::ConsequenceDerivation::Explicit,
                        provenance: provenance.id,
                    },
                ],
                retract: Vec::new(),
            },
        },
        None,
    )?;
    let query = Query::new(&store, generation);
    let seed = LineageEndpoint::ChangeEvent(event.id);
    let complete = query.consequence_neighborhood(&[seed, seed], 2, 3, 2, 20)?;
    let depth_limited = query.consequence_neighborhood(&[seed], 1, 3, 2, 20)?;
    let endpoint_limited = query.consequence_neighborhood(&[seed], 2, 2, 2, 20)?;
    let edge_limited = query.consequence_neighborhood(&[seed], 2, 3, 1, 20)?;
    let scan_limited = query.consequence_neighborhood(&[seed], 2, 3, 2, 1)?;
    let reverse =
        query.consequence_neighborhood(&[LineageEndpoint::FactVersion(second_ref)], 2, 3, 2, 20)?;
    let range_endpoint = LineageEndpoint::FactVersion(first_ref);
    let range_first =
        query.consequences_for_endpoint_range(range_endpoint, generation, generation, None, 1)?;
    let range_cursor = range_first.next_cursor.ok_or(QueryError::InvalidLimit)?;
    let range_second = query.consequences_for_endpoint_range(
        range_endpoint,
        generation,
        generation,
        Some(range_cursor),
        1,
    )?;
    if complete.edges.len() != 2
        || complete.edges.first().map(|item| item.depth) != Some(1)
        || complete.edges.get(1).map(|item| item.depth) != Some(2)
        || complete.endpoints.len() != 3
        || complete.scanned_incidences != 3
        || complete.truncated
        || depth_limited.edges.len() != 1
        || depth_limited.scanned_incidences != 1
        || depth_limited.truncated
        || endpoint_limited.edges.len() != 1
        || !endpoint_limited.truncated
        || edge_limited.edges.len() != 1
        || !edge_limited.truncated
        || scan_limited.edges.len() != 1
        || scan_limited.scanned_incidences != 1
        || !scan_limited.truncated
        || range_first.items.len() != 1
        || range_first
            .items
            .first()
            .is_none_or(|item| item.valid_from != generation || item.valid_until.is_some())
        || range_second.items.len() != 1
        || range_second.next_cursor.is_some()
        || reverse.edges.len() != 2
        || reverse.edges.first().map(|item| item.depth) != Some(1)
        || Query::new(&store, generation)
            .consequence_neighborhood(&[], 1, 1, 1, 1)
            .is_ok()
        || query
            .consequence_neighborhood(&[seed], MAX_CONSEQUENCE_NEIGHBORHOOD_HOPS + 1, 1, 1, 1)
            .is_ok()
        || query
            .consequence_neighborhood(&[seed], 1, 1, 1, MAX_CONSEQUENCE_NEIGHBORHOOD_SCANS + 1)
            .is_ok()
    {
        return Err(QueryError::InvalidLimit);
    }
    Ok(())
}

#[test]
fn consequence_neighborhood_pages_high_degree_endpoints_within_scan_budget()
-> Result<(), QueryError> {
    let repository = RepositoryId::derive(&[b"consequence-page-repo"]);
    let worktree = WorktreeId::derive(&[b"consequence-page-worktree"]);
    let generation = GenerationId::derive(&[b"consequence-page-generation"]);
    let run_id = IndexRunId::derive(&[b"consequence-page-run"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"consequence-page-provenance"]),
        producer_namespace: "test.consequence-page".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let nodes = (0_u64..1_001)
        .map(|index| Node {
            id: NodeId::derive(&[b"consequence-page-node", &index.to_le_bytes()]),
            kind: NodeKind::Function,
            name: format!("target-{index}"),
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        })
        .collect::<Vec<_>>();
    let graph = GraphDelta {
        repository,
        worktree,
        run_id,
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance.clone()],
        upsert_nodes: nodes.clone(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    let mut store = InMemoryGraphStore::new();
    let event = store.change_event_for_delta(&graph)?;
    let consequences = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let target = FactVersionRef {
                fact: FactRef::Node(node.id),
                valid_from: generation,
            };
            ConsequenceEdge {
                id: syntaxmesh_core::ConsequenceEdgeId::derive(&[
                    b"consequence-page-edge",
                    &u64::try_from(index).unwrap_or(u64::MAX).to_le_bytes(),
                ]),
                source: LineageEndpoint::ChangeEvent(event.id),
                target: LineageEndpoint::FactVersion(target),
                kind: syntaxmesh_core::ConsequenceKind::DirectDependencyEffect,
                evidence: vec![target],
                derivation: syntaxmesh_core::ConsequenceDerivation::Explicit,
                provenance: provenance.id,
            }
        })
        .collect();
    store.apply_delta_with_consequences(
        syntaxmesh_core::GraphDeltaWithConsequences {
            publication: syntaxmesh_core::GraphDeltaWithLineage {
                graph,
                lineage: syntaxmesh_core::ChangeSetDelta::default(),
            },
            consequences: syntaxmesh_core::ConsequenceDelta {
                add: consequences,
                retract: Vec::new(),
            },
        },
        None,
    )?;

    let result = Query::new(&store, generation).consequence_neighborhood(
        &[LineageEndpoint::ChangeEvent(event.id)],
        1,
        1_002,
        1_001,
        1_001,
    )?;
    if result.edges.len() != 1_001
        || result.endpoints.len() != 1_002
        || result.scanned_incidences != 1_001
        || result.truncated
    {
        return Err(QueryError::InvalidLimit);
    }
    Ok(())
}

#[test]
fn historical_neighborhood_deduplicates_cycle_edges() -> Result<(), QueryError> {
    let repository = RepositoryId::derive(&[b"historical-cycle-repo"]);
    let worktree = WorktreeId::derive(&[b"historical-cycle-worktree"]);
    let generation = GenerationId::derive(&[b"historical-cycle-generation"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"historical-cycle-provenance"]),
        producer_namespace: "test.historical-cycle".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let provenance_id = provenance.id;
    let node = |name: &'static str| Node {
        id: NodeId::derive(&[name.as_bytes()]),
        kind: NodeKind::Function,
        name: name.to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance_id,
        extension_payload: None,
    };
    let first = node("historical-cycle-first");
    let second = node("historical-cycle-second");
    let edge = |name: &'static str, source, target| Edge {
        id: EdgeId::derive(&[name.as_bytes()]),
        source,
        target,
        relation: RelationKind::Calls,
        provenance: provenance_id,
        extension_payload: None,
    };
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"historical-cycle-run"]),
        expected_base: None,
        next_generation: generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![first.clone(), second.clone()],
        upsert_edges: vec![
            edge("historical-cycle-edge-a", first.id, second.id),
            edge("historical-cycle-edge-b", second.id, first.id),
        ],
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;

    let result = Query::new(&store, generation).historical_neighborhood(
        &[first.id],
        8,
        2,
        2,
        8,
        MAX_HISTORICAL_NEIGHBORHOOD_RESULT_BYTES,
    )?;
    if result.nodes.len() != 2 || result.edges.len() != 2 || result.truncated {
        return Err(QueryError::InvalidLimit);
    }
    Ok(())
}

#[test]
fn searches_and_traverses_one_generation() -> Result<(), QueryError> {
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
    let first = Node {
        id: NodeId::derive(&[b"first"]),
        kind: NodeKind::Function,
        name: "first".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let second = Node {
        id: NodeId::derive(&[b"second"]),
        kind: NodeKind::Function,
        name: "second".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let third = Node {
        id: NodeId::derive(&[b"third"]),
        kind: NodeKind::Function,
        name: "third".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let fourth = Node {
        id: NodeId::derive(&[b"fourth"]),
        kind: NodeKind::Function,
        name: "fourth".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let fourth_id = fourth.id;
    let edge = Edge {
        id: EdgeId::derive(&[b"edge"]),
        source: first.id,
        target: second.id,
        relation: RelationKind::Calls,
        provenance: provenance.id,
        extension_payload: None,
    };
    let second_edge = Edge {
        id: EdgeId::derive(&[b"second-edge"]),
        source: first.id,
        target: third.id,
        relation: RelationKind::Calls,
        provenance: provenance.id,
        extension_payload: None,
    };
    let third_edge = Edge {
        id: EdgeId::derive(&[b"third-edge"]),
        source: third.id,
        target: fourth.id,
        relation: RelationKind::Calls,
        provenance: provenance.id,
        extension_payload: None,
    };
    let mut store = InMemoryGraphStore::new();
    let manifest = store.apply_delta_at(
        GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"run"]),
            expected_base: None,
            next_generation: generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: vec![provenance],
            upsert_nodes: vec![first.clone(), second.clone(), third, fourth.clone()],
            upsert_edges: vec![edge, second_edge, third_edge],
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        },
        AcceptanceTime(10),
    )?;
    let query = Query::new(&store, manifest.generation);
    let stats = crate::generation_term_statistics(&store, generation, "first second absent", 4)?;
    let partial = crate::generation_term_statistics(&store, generation, "first", 1)?;
    if !stats.complete
        || stats.scanned_nodes != 4
        || stats.frequencies.get("first") != Some(&1)
        || stats.frequencies.get("second") != Some(&1)
        || stats.frequencies.get("absent") != Some(&0)
        || partial.complete
        || partial.scanned_nodes != 1
        || crate::generation_term_statistics(&store, generation, "first", 0).is_ok()
    {
        return Err(QueryError::Serialization(
            "term statistics lost exactness or budget status".to_owned(),
        ));
    }
    let edge_history = query.fact_history(FactRef::Edge(EdgeId::derive(&[b"edge"])))?;
    let events = query.change_events_for_fact(FactRef::Node(first.id), None, 1)?;
    let direct_event = query.change_event()?;
    let historical_page = query.historical_neighbors(first.id, EdgeDirection::Outgoing, 1, None)?;
    let page_records = historical_page.clone().into_records();
    if page_records
        != query.historical_neighbor_records(first.id, EdgeDirection::Outgoing, 1, None)?
    {
        return Err(QueryError::Serialization(
            "page export differs from query export".to_owned(),
        ));
    }
    if !matches!(page_records.last(), Some(TemporalRecord::HistoricalNeighborFooter {
        generation: selected, endpoint, direction: EdgeDirection::Outgoing,
        returned: 1, has_more: true, next_cursor,
        ..
    }) if *selected == generation && *endpoint == first.id && *next_cursor == historical_page.next_cursor)
    {
        return Err(QueryError::Serialization(
            "page export lost continuation metadata".to_owned(),
        ));
    }
    for record in &page_records {
        let line = record
            .to_json_line()
            .map_err(|error| QueryError::Serialization(error.to_string()))?;
        let decoded: TemporalRecord = serde_json::from_str(&line)
            .map_err(|error| QueryError::Serialization(error.to_string()))?;
        if &decoded != record {
            return Err(QueryError::Serialization(
                "page record round trip differs".to_owned(),
            ));
        }
    }
    let next_neighbor_page = query.historical_neighbors(
        first.id,
        EdgeDirection::Outgoing,
        1,
        historical_page.next_cursor,
    )?;
    let output_cap = MAX_HISTORICAL_NEIGHBORHOOD_RESULT_BYTES;
    let historical =
        query.historical_neighborhood(&[first.id, first.id], 2, 4, 3, 20, output_cap)?;
    let hop_bounded = query.historical_neighborhood(&[first.id], 1, 4, 3, 20, output_cap)?;
    let historical_node_bounded =
        query.historical_neighborhood(&[first.id], 2, 2, 3, 20, output_cap)?;
    let historical_edge_bounded =
        query.historical_neighborhood(&[first.id], 2, 4, 1, 20, output_cap)?;
    let scan_bounded = query.historical_neighborhood(&[first.id], 2, 4, 3, 1, output_cap)?;
    let byte_bounded = query.historical_neighborhood(&[first.id], 2, 4, 3, 20, 1024)?;
    let byte_records = query.historical_neighborhood_records(&[first.id], 2, 4, 3, 20, 1024)?;
    let encoded_bytes = byte_records.iter().try_fold(0_usize, |total, record| {
        record
            .to_json_line()
            .map(|line| total.saturating_add(line.len()).saturating_add(1))
            .map_err(|error| QueryError::Serialization(error.to_string()))
    })?;
    let encoded_item_bytes = byte_records
        .iter()
        .take(byte_records.len().saturating_sub(1))
        .try_fold(0_usize, |total, record| {
            record
                .to_json_line()
                .map(|line| total.saturating_add(line.len()).saturating_add(1))
                .map_err(|error| QueryError::Serialization(error.to_string()))
        })?;
    let mut historical_depths = historical
        .edges
        .iter()
        .map(|item| item.depth)
        .collect::<Vec<_>>();
    historical_depths.sort_unstable();
    if query.search("FIRST", 10)?.len() != 1
        || historical.nodes.len() != 4
        || historical.edges.len() != 3
        || historical_depths != [1, 1, 2]
        || historical.truncated
        || hop_bounded.edges.len() != 2
        || hop_bounded.truncated
        || historical_node_bounded.nodes.len() != 2
        || !historical_node_bounded.truncated
        || historical_edge_bounded.edges.len() != 1
        || !historical_edge_bounded.truncated
        || scan_bounded.edges.len() != 1
        || !scan_bounded.truncated
        || !byte_bounded.truncated
        || byte_bounded.nodes.first().map(|node| (node.id, node.depth)) != Some((first.id, 0))
        || byte_bounded.serialized_item_bytes != encoded_item_bytes
        || encoded_bytes > 1024
        || !matches!(
            byte_records.last(),
            Some(TemporalRecord::HistoricalNeighborhoodFooter {
                serialized_item_bytes,
                truncated: true,
                ..
            }) if usize::try_from(*serialized_item_bytes).ok() == Some(encoded_item_bytes)
        )
        || !matches!(
            query.historical_neighborhood(&[first.id], 1, 1, 1, 1, 1),
            Err(QueryError::OutputBudgetTooSmall { .. })
        )
        || query
            .historical_neighborhood(
                &[first.id],
                1,
                1,
                1,
                1,
                MAX_HISTORICAL_NEIGHBORHOOD_RESULT_BYTES + 1,
            )
            .is_ok()
        || query
            .historical_neighborhood(&[], 1, 1, 1, 1, output_cap)
            .is_ok()
        || query
            .historical_neighborhood(&[first.id], 0, 1, 1, 1, output_cap)
            .is_ok()
        || query
            .historical_neighborhood(&[first.id], 1, 1, 1, 0, output_cap)
            .is_ok()
        || !matches!(
            query.historical_neighborhood(
                &[NodeId::derive(&[b"historical-missing-seed"])],
                1,
                1,
                1,
                1,
                output_cap,
            ),
            Err(QueryError::UnknownSeed(_))
        )
        || !matches!(direct_event, Some(TemporalRecord::ChangeEvent { event, .. }) if event.generation_after == generation)
        || !matches!(
            events.as_slice(),
            [
                TemporalRecord::ChangeEvent {
                    schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                    event,
                    ..
                },
                TemporalRecord::ChangeEventFooter {
                    returned: 1,
                    has_more: false,
                    next_cursor: None,
                    ..
                }
            ] if event.generation_after == generation
                && event.changed_facts.iter().any(|changed| {
                    changed.fact == FactRef::Node(first.id)
                        && changed.kind == syntaxmesh_core::FactChangeKind::Added
                })
        )
        || !matches!(edge_history.as_slice(), [TemporalRecord::FactVersion { payload: syntaxmesh_core::FactPayload::Edge(history_edge), valid_from, valid_until: None, .. }] if history_edge.id == EdgeId::derive(&[b"edge"]) && *valid_from == generation)
        || query.neighbors(first.id)?.len() != 2
        || historical_page.items.len() != 1
        || !historical_page.has_more
        || historical_page.next_cursor.is_none()
        || next_neighbor_page.items.len() != 1
        || next_neighbor_page.has_more
        || next_neighbor_page.next_cursor.is_some()
        || historical_page.items.first().map(|item| item.edge.id)
            == next_neighbor_page.items.first().map(|item| item.edge.id)
        || query
            .historical_neighbors(
                first.id,
                EdgeDirection::Incoming,
                1,
                historical_page.next_cursor,
            )
            .is_ok()
        || query.path(first.id, second.id, 2)? != Some(vec![first.id, second.id])
        || query.impact(second.id, 1)? != vec![first.id]
    {
        return Err(QueryError::InvalidLimit);
    }
    let records = query.export_records()?;
    if !matches!(
        records.first(),
        Some(GraphRecord::Header {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            ..
        })
    ) || !matches!(
        records.last(),
        Some(GraphRecord::Footer {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            nodes: 4,
            edges: 3,
            provenance_records: 1,
            truncated: false,
            ..
        })
    ) {
        return Err(QueryError::InvalidLimit);
    }
    let node_bounded = query.export_subgraph(&[first.id], 2, 2, 4)?;
    let edge_bounded = query.export_subgraph(&[first.id], 2, 4, 1)?;
    let depth_bounded = query.export_subgraph(&[first.id], 1, 4, 3)?;
    let same_neighborhood = query.export_subgraph(&[first.id, first.id], 2, 2, 4)?;
    if node_bounded != same_neighborhood
        || !matches!(
            node_bounded.last(),
            Some(GraphRecord::Footer {
                nodes: 2,
                edges: 1,
                truncated: true,
                ..
            })
        )
        || !matches!(
            edge_bounded.last(),
            Some(GraphRecord::Footer {
                nodes: 4,
                edges: 1,
                truncated: true,
                ..
            })
        )
        || !matches!(
            depth_bounded.last(),
            Some(GraphRecord::Footer {
                nodes: 3,
                edges: 2,
                truncated: false,
                ..
            })
        )
        || query.export_subgraph(&[], 1, 1, 1).is_ok()
        || query.export_subgraph(&[first.id], 0, 1, 1).is_ok()
        || query.export_subgraph(&[first.id], 1, 0, 1).is_ok()
        || query
            .export_subgraph(&[NodeId::derive(&[b"missing"])], 1, 1, 1)
            .is_ok()
    {
        return Err(QueryError::InvalidLimit);
    }
    let next_generation = GenerationId::derive(&[b"generation-two"]);
    store.apply_delta_at(
        GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"run-two"]),
            expected_base: Some(manifest.generation),
            next_generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: vec![Node {
                name: "fourth-updated".to_owned(),
                ..fourth.clone()
            }],
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        },
        AcceptanceTime(10),
    )?;
    let timeline = Query::new(&store, next_generation);
    let graph_at = timeline.export_graph_at(manifest.generation)?;
    let history = timeline.history(first.id)?;
    let fact_lineage = timeline.fact_lineage(FactRef::Node(fourth.id))?;
    let changes = timeline.changed_between(manifest.generation, next_generation)?;
    let accepted = timeline.accepted_between(AcceptanceTime(10), AcceptanceTime(11), None, 10)?;
    let limited_accepted =
        timeline.accepted_between(AcceptanceTime(10), AcceptanceTime(21), None, 1)?;
    let cursor = limited_accepted.iter().find_map(|record| match record {
        TemporalRecord::AcceptanceTimelineFooter {
            next_cursor: Some(cursor),
            ..
        } => Some(*cursor),
        TemporalRecord::AcceptedGeneration { .. }
        | TemporalRecord::HistoricalNeighborhoodNode { .. }
        | TemporalRecord::HistoricalNeighborhoodEdge { .. }
        | TemporalRecord::HistoricalNeighborhoodFooter { .. }
        | TemporalRecord::NodeVersion { .. }
        | TemporalRecord::Change { .. }
        | TemporalRecord::FactVersion { .. }
        | TemporalRecord::FactSupersedes { .. }
        | TemporalRecord::ChangeEvent { .. }
        | TemporalRecord::ChangeEventFooter { .. }
        | TemporalRecord::ChangeEventCorrelation { .. }
        | TemporalRecord::ChangeEventCorrelationFooter { .. }
        | TemporalRecord::ChangeSetEvent { .. }
        | TemporalRecord::ChangeSetEventsFooter { .. }
        | TemporalRecord::ChangeSetVersion { .. }
        | TemporalRecord::ConsequenceEdge { .. }
        | TemporalRecord::ConsequenceNeighborhoodFooter { .. }
        | TemporalRecord::ConsequenceTraceHeader { .. }
        | TemporalRecord::ConsequenceTraceState { .. }
        | TemporalRecord::ConsequenceTraceHop { .. }
        | TemporalRecord::ConsequenceTraceFooter { .. }
        | TemporalRecord::ObservedFact { .. }
        | TemporalRecord::ObservedTimelineFooter { .. }
        | TemporalRecord::AcceptanceTimelineFooter {
            next_cursor: None, ..
        } => None,
        TemporalRecord::HistoricalNeighbor { .. }
        | TemporalRecord::HistoricalNeighborFooter { .. } => None,
    });
    let next_accepted =
        timeline.accepted_between(AcceptanceTime(10), AcceptanceTime(21), cursor, 1)?;
    if history.len() != 1
        || !matches!(history.first(), Some(TemporalRecord::NodeVersion { schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION, query_mode: TemporalQueryMode::HistoricalConclusion, valid_from, node, .. }) if *valid_from == manifest.generation && node.id == first.id)
        || changes.len() != 1
        || !matches!(
            fact_lineage.as_slice(),
            [TemporalRecord::FactSupersedes {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                prior,
                current,
                accepted_at: Some(AcceptanceTime(10)),
            }]
            if prior.fact == FactRef::Node(fourth.id)
                && prior.valid_from == manifest.generation
                && current.fact == FactRef::Node(fourth.id)
                && current.valid_from == next_generation
        )
        || !matches!(changes.first(), Some(TemporalRecord::Change { schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION, query_mode: TemporalQueryMode::HistoricalConclusion, manifest: change_manifest, .. }) if change_manifest.generation == next_generation)
        || accepted.len() != 3
        || !matches!(
            accepted.first(),
            Some(TemporalRecord::AcceptedGeneration {
                schema_version: ACCEPTANCE_TIMELINE_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::AcceptanceTimeline,
                accepted_at: AcceptanceTime(10),
                ..
            })
        )
        || !matches!(
            accepted.get(1),
            Some(TemporalRecord::AcceptedGeneration {
                schema_version: ACCEPTANCE_TIMELINE_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::AcceptanceTimeline,
                accepted_at: AcceptanceTime(10),
                ..
            })
        )
        || !matches!(
            accepted.last(),
            Some(TemporalRecord::AcceptanceTimelineFooter {
                query_mode: TemporalQueryMode::AcceptanceTimeline,
                returned: 2,
                has_more: false,
                next_cursor: None,
                ..
            })
        )
        || limited_accepted.len() != 2
        || !matches!(
            limited_accepted.first(),
            Some(TemporalRecord::AcceptedGeneration {
                accepted_at: AcceptanceTime(10),
                ..
            })
        )
        || !matches!(
            limited_accepted.last(),
            Some(TemporalRecord::AcceptanceTimelineFooter {
                returned: 1,
                has_more: true,
                next_cursor: Some(_),
                ..
            })
        )
        || !matches!(
            next_accepted.first(),
            Some(TemporalRecord::AcceptedGeneration {
                accepted_at: AcceptanceTime(10),
                ..
            })
        )
        || !matches!(
            next_accepted.last(),
            Some(TemporalRecord::AcceptanceTimelineFooter {
                has_more: false,
                next_cursor: None,
                ..
            })
        )
        || timeline
            .accepted_between(AcceptanceTime(10), AcceptanceTime(10), None, 1)
            .is_ok()
        || timeline
            .accepted_between(AcceptanceTime(10), AcceptanceTime(11), None, 0)
            .is_ok()
        || !matches!(graph_at.first(), Some(GraphRecord::Header { query_mode: TemporalQueryMode::HistoricalConclusion, generation: value, .. }) if *value == manifest.generation)
        || !matches!(
            graph_at.last(),
            Some(GraphRecord::Footer {
                nodes: 4,
                edges: 3,
                ..
            })
        )
    {
        return Err(QueryError::InvalidLimit);
    }

    let removed_generation = GenerationId::derive(&[b"generation-three"]);
    store.apply_delta_at(
        GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"run-three"]),
            expected_base: Some(next_generation),
            next_generation: removed_generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: Vec::new(),
            upsert_edges: Vec::new(),
            remove_nodes: vec![fourth.id],
            remove_edges: Vec::new(),
        },
        AcceptanceTime(11),
    )?;
    let cascaded_edge_history = Query::new(&store, removed_generation)
        .fact_history(FactRef::Edge(EdgeId::derive(&[b"third-edge"])))?;
    if !matches!(
        cascaded_edge_history.as_slice(),
        [TemporalRecord::FactVersion {
            valid_from: from,
            valid_until: Some(until),
            payload: syntaxmesh_core::FactPayload::Edge(history_edge),
            ..
        }]
            if *from == manifest.generation
                && *until == removed_generation
                && history_edge.id == EdgeId::derive(&[b"third-edge"])
    ) {
        return Err(QueryError::InvalidLimit);
    }
    let reintroduced_generation = GenerationId::derive(&[b"generation-four"]);
    store.apply_delta_at(
        GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"run-four"]),
            expected_base: Some(removed_generation),
            next_generation: reintroduced_generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes: vec![Node {
                name: "fourth-reintroduced".to_owned(),
                ..fourth
            }],
            upsert_edges: Vec::new(),
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        },
        AcceptanceTime(12),
    )?;
    let reintroduced =
        Query::new(&store, reintroduced_generation).fact_lineage(FactRef::Node(fourth_id))?;
    if reintroduced.len() != 1 {
        return Err(QueryError::InvalidLimit);
    }
    Ok(())
}

#[test]
fn historical_node_reads_one_generation_without_current_projection_lookup() -> Result<(), QueryError>
{
    let repository = RepositoryId::derive(&[b"historical-node-repository"]);
    let worktree = WorktreeId::derive(&[b"historical-node-worktree"]);
    let first_generation = GenerationId::derive(&[b"historical-node-first"]);
    let second_generation = GenerationId::derive(&[b"historical-node-second"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"historical-node-provenance"]),
        producer_namespace: "test.historical-node".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let historical_target = Node {
        id: NodeId::derive(&[b"historical-node-target"]),
        kind: NodeKind::Function,
        name: "historical_target".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let node_id = historical_target.id;
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"historical-node-run-first"]),
        expected_base: None,
        next_generation: first_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![historical_target],
        remove_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"historical-node-run-second"]),
        expected_base: Some(first_generation),
        next_generation: second_generation,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: Vec::new(),
        remove_nodes: vec![node_id],
        upsert_edges: Vec::new(),
        remove_edges: Vec::new(),
    })?;

    let first_node = Query::new(&store, first_generation).historical_node(node_id)?;
    if first_node.as_ref().map(|version| version.name.as_str()) != Some("historical_target") {
        return Err(QueryError::Context(
            "historical point lookup missed the original node version".to_owned(),
        ));
    }
    if Query::new(&store, second_generation)
        .historical_node(node_id)?
        .is_some()
    {
        return Err(QueryError::Context(
            "historical point lookup returned a node after its removal".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn correlated_change_events_are_non_causal_and_cursor_bounded() -> Result<(), QueryError> {
    let repository = RepositoryId::derive(&[b"correlation-repository"]);
    let worktree = WorktreeId::derive(&[b"correlation-worktree"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"correlation-provenance"]),
        producer_namespace: "syntaxmesh.correlation-test".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let fact = FactRef::Node(NodeId::derive(&[b"correlated-fact"]));
    let node_id = match fact {
        FactRef::Node(id) => id,
        FactRef::File(_) | FactRef::Provenance(_) | FactRef::Edge(_) => {
            return Err(QueryError::InvalidCorrelationAnchor);
        }
    };
    let mut store = InMemoryGraphStore::new();
    let mut previous = None;
    let mut generations = Vec::new();
    for index in 0..5_u8 {
        let generation = GenerationId::derive(&[b"correlation-generation", &[index]]);
        let deleting = index == 2;
        let no_op = index == 4;
        let node = Node {
            id: node_id,
            kind: NodeKind::Function,
            name: format!("version-{index}"),
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        };
        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"correlation-run", &[index]]),
            expected_base: previous,
            next_generation: generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: (index == 0)
                .then(|| provenance.clone())
                .into_iter()
                .collect(),
            upsert_nodes: if deleting || no_op {
                Vec::new()
            } else {
                vec![node]
            },
            upsert_edges: Vec::new(),
            remove_nodes: if deleting { vec![node_id] } else { Vec::new() },
            remove_edges: Vec::new(),
        })?;
        generations.push(generation);
        previous = Some(generation);
    }

    let earlier_generation = *generations
        .first()
        .ok_or(QueryError::InvalidCorrelationAnchor)?;
    let source_generation = *generations
        .get(1)
        .ok_or(QueryError::InvalidCorrelationAnchor)?;
    let first_target_generation = *generations
        .get(2)
        .ok_or(QueryError::InvalidCorrelationAnchor)?;
    let second_target_generation = *generations
        .get(3)
        .ok_or(QueryError::InvalidCorrelationAnchor)?;
    let no_op_generation = *generations
        .get(4)
        .ok_or(QueryError::InvalidCorrelationAnchor)?;
    let source = Query::new(&store, source_generation);
    let first_page = source.change_event_correlations(fact, None, 1)?;
    let cursor = match first_page.last() {
        Some(TemporalRecord::ChangeEventCorrelationFooter {
            has_more: true,
            next_cursor: Some(cursor),
            ..
        }) => *cursor,
        _ => return Err(QueryError::InvalidCorrelationAnchor),
    };
    let second_page = source.change_event_correlations(fact, Some(cursor), 1)?;
    let source_event = store
        .change_event(source_generation)?
        .ok_or(QueryError::InvalidCorrelationAnchor)?;
    let first_target = store
        .change_event(first_target_generation)?
        .ok_or(QueryError::InvalidCorrelationAnchor)?;
    let second_target = store
        .change_event(second_target_generation)?
        .ok_or(QueryError::InvalidCorrelationAnchor)?;
    if !matches!(
        first_page.first(),
        Some(TemporalRecord::ChangeEventCorrelation { correlation, .. })
            if correlation.source_event == source_event.id
                && correlation.kind
                    == syntaxmesh_core::ChangeEventRelationKind::HistoricallyCorrelated
                && correlation.target_event == first_target.id
                && correlation.target_generation == first_target_generation
                && correlation.shared_fact == fact
    ) || !matches!(
        second_page.as_slice(),
        [
            TemporalRecord::ChangeEventCorrelation { correlation, .. },
            TemporalRecord::ChangeEventCorrelationFooter {
                has_more: false,
                next_cursor: None,
                ..
            }
        ] if correlation.source_event == source_event.id
            && correlation.target_event == second_target.id
            && correlation.target_generation == second_target_generation
            && correlation.shared_fact == fact
    ) || !matches!(
        source.change_event_correlations(
            FactRef::Node(NodeId::derive(&[b"unrelated-fact"])),
            None,
            1
        ),
        Err(QueryError::InvalidCorrelationAnchor)
    ) || !matches!(
        source.change_event_correlations(
            fact,
            Some(ChangeEventCorrelationCursor {
                source_event: source_event.id,
                after_generation: earlier_generation,
            }),
            1
        ),
        Err(QueryError::InvalidCorrelationAnchor)
    ) || !matches!(
        source.change_event_correlations(
            fact,
            Some(ChangeEventCorrelationCursor {
                source_event: source_event.id,
                after_generation: no_op_generation,
            }),
            1
        ),
        Err(QueryError::InvalidCorrelationAnchor)
    ) || !matches!(
        source.change_event_correlations(
            fact,
            Some(ChangeEventCorrelationCursor {
                source_event: syntaxmesh_core::ChangeEventId::derive(&[b"wrong-source"]),
                after_generation: first_target_generation,
            }),
            1
        ),
        Err(QueryError::InvalidCorrelationAnchor)
    ) {
        return Err(QueryError::InvalidCorrelationAnchor);
    }
    Ok(())
}

#[test]
fn explicit_change_set_query_returns_evidence_and_snapshot_bound_footer() -> Result<(), QueryError>
{
    let repository = RepositoryId::derive(&[b"changeset-query-repo"]);
    let worktree = WorktreeId::derive(&[b"changeset-query-worktree"]);
    let first = GenerationId::derive(&[b"changeset-query-first"]);
    let second = GenerationId::derive(&[b"changeset-query-second"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"changeset-query-provenance"]),
        producer_namespace: "test.changeset".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::SourceFact,
        source: None,
    };
    let change_set = syntaxmesh_core::ChangeSetId::derive(&[b"changeset-query-id"]);
    let mut store = InMemoryGraphStore::new();
    let mut initial = GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"changeset-query-run-1"]),
        expected_base: None,
        next_generation: first,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance.clone()],
        upsert_nodes: Vec::new(),
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    };
    store.apply_delta(initial.clone())?;
    initial.upsert_provenance.clear();
    let graph = GraphDelta {
        expected_base: Some(first),
        next_generation: second,
        run_id: IndexRunId::derive(&[b"changeset-query-run-2"]),
        ..initial
    };
    let event = store.change_event_for_delta(&graph)?;
    store.apply_delta_with_lineage(
        syntaxmesh_core::GraphDeltaWithLineage {
            graph,
            lineage: syntaxmesh_core::ChangeSetDelta {
                upsert_sets: vec![syntaxmesh_core::ChangeSet {
                    id: change_set,
                    kind: syntaxmesh_core::ChangeSetKind::ManualGroup,
                    title: None,
                    originating_intent: None,
                    parent_changes: Vec::new(),
                    git_commits: Vec::new(),
                    pull_requests: Vec::new(),
                    issues: Vec::new(),
                    adrs: Vec::new(),
                    repositories: vec![repository],
                    first_generation: first,
                    last_generation: Some(second),
                    provenance: provenance.id,
                }],
                assign_events: vec![syntaxmesh_core::ChangeSetMembership {
                    change_set,
                    event: event.id,
                    provenance: provenance.id,
                }],
                unassign_events: Vec::new(),
            },
        },
        None,
    )?;
    let records = Query::new(&store, second).events_for_change_set(change_set, None, 1)?;
    let declaration = Query::new(&store, second).change_set_at(change_set)?;
    if !matches!(records.as_slice(), [
        TemporalRecord::ChangeSetEvent { item, .. },
        TemporalRecord::ChangeSetEventsFooter { returned: 1, has_more: false, next_cursor: None, .. }
    ] if item.membership.change_set == change_set && item.event.id == event.id && item.membership.provenance == provenance.id)
        || !matches!(declaration, Some(TemporalRecord::ChangeSetVersion { change_set: set, valid_from, valid_until: None, .. }) if set.id == change_set && valid_from == second)
    {
        return Err(QueryError::InvalidLimit);
    }
    Ok(())
}
