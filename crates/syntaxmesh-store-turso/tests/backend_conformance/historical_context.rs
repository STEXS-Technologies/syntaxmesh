//! Temporal context reuses the existing context fixture and backend factories.

use syntaxmesh_query::QueryError;

use super::{
    ContextFixtureCounter, ContextFixtureSource, ContextItemKind, ContextPack, ContextRequest,
    EvidenceClass, FileGraphStore, GenerationId, GraphDelta, GraphStore, InMemoryGraphStore,
    IndexRunId, NodeId, Query, SqliteGraphStore, StoreError, context_pack, open_migrated,
    open_turso, publish_context_fixture, repository, worktree,
};

const SOURCE: &[u8] = b"pub fn caller() { helper(); }\npub fn helper() {}\n";
const REVISED_SOURCE: &[u8] = b"pub fn caller() { helper(); }\npub fn helper() {}\n// later\n";

#[path = "historical_context/packing_plan.rs"]
mod packing_plan;

fn occurrence_ids() -> Vec<NodeId> {
    let caller = NodeId::derive(&[b"context-conformance-caller"]);
    (0_u64..1000)
        .map(|number| NodeId::derive(&[b"module-occurrence-tie", &number.to_le_bytes()]))
        .filter(|id| *id < caller)
        .take(3)
        .collect()
}

fn occurrence_delta(store: &dyn GraphStore, base: GenerationId) -> Result<GraphDelta, StoreError> {
    let caller = NodeId::derive(&[b"context-conformance-caller"]);
    let template = store
        .historical_node(base, caller)?
        .ok_or_else(|| StoreError::Integrity("missing occurrence source fixture".to_owned()))?;
    let ids = occurrence_ids();
    let import = ids
        .first()
        .copied()
        .ok_or_else(|| StoreError::Integrity("missing occurrence ID".to_owned()))?;
    if ids.len() != 3 {
        return Err(StoreError::Integrity(
            "insufficient lower canonical occurrence IDs".to_owned(),
        ));
    }
    let kinds = [
        syntaxmesh_core::NodeKind::Import {
            specifier: "./module".to_owned(),
            kind: syntaxmesh_core::ImportKind::Named,
            imported_name: Some("symbol".to_owned()),
            local_name: Some("symbol".to_owned()),
            type_only: false,
        },
        syntaxmesh_core::NodeKind::Export {
            source_specifier: None,
            kind: syntaxmesh_core::ExportKind::Local,
            exported_name: Some("symbol".to_owned()),
            local_name: Some("symbol".to_owned()),
            type_only: false,
        },
        syntaxmesh_core::NodeKind::ModuleResolutionDiagnostic {
            occurrence: import,
            status: syntaxmesh_core::ModuleResolutionDiagnosticStatus::Unresolved,
            candidate_paths: Vec::new(),
        },
    ];
    let mut delta = saturation_delta(base);
    delta.run_id = IndexRunId::derive(&[b"module-occurrence-tie-run"]);
    delta.next_generation = GenerationId::derive(&[b"module-occurrence-tie-generation"]);
    delta.upsert_edges.clear();
    delta.upsert_nodes = ids
        .into_iter()
        .zip(kinds)
        .map(|(id, kind)| syntaxmesh_core::Node {
            id,
            kind,
            ..template.clone()
        })
        .collect();
    delta.upsert_nodes.extend(packing_plan::nodes(&template));
    Ok(delta)
}

fn occurrence_pack(
    store: &dyn GraphStore,
    generation: GenerationId,
    historical: bool,
) -> Result<ContextPack, StoreError> {
    let caller = NodeId::derive(&[b"context-conformance-caller"]);
    let mut plan = request(true);
    plan.seed_nodes = occurrence_ids();
    plan.seed_nodes.push(caller);
    plan.max_hops = 0;
    plan.token_budget = 20_000;
    let query = Query::new(store, generation);
    let source = ContextFixtureSource(SOURCE.to_vec());
    let result = if historical {
        query.historical_seeded_context(&plan, &source, &ContextFixtureCounter)
    } else {
        query.seeded_context(&plan, &source, &ContextFixtureCounter)
    };
    let pack = result.map_err(|error| StoreError::Backend(error.to_string()))?;
    let evidence = pack
        .items
        .iter()
        .filter(|item| item.kind == ContextItemKind::SourceEvidence)
        .collect::<Vec<_>>();
    if evidence.len() != 1
        || evidence
            .first()
            .is_none_or(|item| !item.node_ids.contains(&caller))
        || occurrence_ids()
            .iter()
            .any(|id| !evidence.iter().any(|item| item.node_ids.contains(id)))
    {
        return Err(StoreError::Integrity(
            "module occurrence tie lost definition precedence or evidence".to_owned(),
        ));
    }
    Ok(pack)
}

fn saturation_delta(base: GenerationId) -> GraphDelta {
    let provenance = syntaxmesh_core::ProvenanceId::derive(&[b"context-conformance-provenance"]);
    let pairs = [
        (
            NodeId::derive(&[b"context-conformance-caller"]),
            NodeId::derive(&[b"ranked-caller-neighbor"]),
        ),
        (
            NodeId::derive(&[b"context-conformance-helper"]),
            NodeId::derive(&[b"ranked-helper-neighbor"]),
        ),
    ];
    GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"ranked-saturation-run"]),
        expected_base: Some(base),
        next_generation: GenerationId::derive(&[b"ranked-saturation-generation"]),
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: Vec::new(),
        upsert_nodes: pairs
            .iter()
            .map(|(_, id)| syntaxmesh_core::Node {
                id: *id,
                kind: syntaxmesh_core::NodeKind::Function,
                name: "ranked_neighbor".to_owned(),
                owner_file: None,
                source: None,
                provenance,
                extension_payload: None,
            })
            .collect(),
        upsert_edges: pairs
            .iter()
            .map(|(source, target)| syntaxmesh_core::Edge {
                id: syntaxmesh_core::EdgeId::derive(&[b"ranked-saturation-edge", &source.0.0]),
                source: *source,
                target: *target,
                relation: syntaxmesh_core::RelationKind::Calls,
                provenance,
                extension_payload: None,
            })
            .collect(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    }
}

fn saturated_ranked_pack(
    store: &dyn GraphStore,
    generation: GenerationId,
    reverse: bool,
    historical: bool,
) -> Result<ContextPack, StoreError> {
    let caller = NodeId::derive(&[b"context-conformance-caller"]);
    let helper = NodeId::derive(&[b"context-conformance-helper"]);
    let mut plan = request(true);
    plan.seed_nodes = if reverse {
        vec![helper, caller]
    } else {
        vec![caller, helper]
    };
    plan.max_candidates = 3;
    let query = Query::new(store, generation);
    let selected = if historical {
        query.historical_ranked_seeded_context_selection(&plan)
    } else {
        query.ranked_seeded_context_selection(&plan)
    }
    .map_err(|error| StoreError::Backend(error.to_string()))?;
    let wanted = NodeId::derive(&[if reverse {
        b"ranked-helper-neighbor"
    } else {
        b"ranked-caller-neighbor"
    }]);
    if selected.nodes.len() != 3 || !selected.nodes.contains(&(wanted, 1, 0)) {
        return Err(StoreError::Integrity(
            "ranked saturated frontier admitted wrong neighbor".to_owned(),
        ));
    }
    let source = ContextFixtureSource(SOURCE.to_vec());
    let packed = if historical {
        query.historical_ranked_seeded_context(&plan, &source, &ContextFixtureCounter)
    } else {
        query.ranked_seeded_context(&plan, &source, &ContextFixtureCounter)
    };
    packed.map_err(|error| StoreError::Backend(error.to_string()))
}

fn request(seeded: bool) -> ContextRequest {
    ContextRequest {
        query: "caller".to_owned(),
        seed_nodes: if seeded {
            vec![NodeId::derive(&[b"context-conformance-caller"])]
        } else {
            Vec::new()
        },
        token_budget: 4096,
        max_hops: 1,
        max_candidates: 16,
    }
}

fn historical_pack(
    store: &dyn GraphStore,
    generation: GenerationId,
    seeded: bool,
) -> Result<ContextPack, StoreError> {
    if seeded {
        return Query::new(store, generation)
            .historical_seeded_context(
                &request(true),
                &ContextFixtureSource(SOURCE.to_vec()),
                &ContextFixtureCounter,
            )
            .map_err(|error| StoreError::Backend(error.to_string()));
    }
    Query::new(store, generation)
        .historical_context(
            &request(seeded),
            &ContextFixtureSource(SOURCE.to_vec()),
            &ContextFixtureCounter,
        )
        .map_err(|error| StoreError::Backend(error.to_string()))
}

fn revise(store: &mut dyn GraphStore, base: GenerationId) -> Result<(), StoreError> {
    let mut snapshot = store.historical_snapshot(base)?;
    let revised_hash = *blake3::hash(REVISED_SOURCE).as_bytes();
    for file in &mut snapshot.files {
        file.content_hash = revised_hash;
        file.size_bytes = u64::try_from(REVISED_SOURCE.len())
            .map_err(|error| StoreError::Backend(error.to_string()))?;
    }
    for node in &mut snapshot.nodes {
        node.name.push_str("_later");
        if let Some(source) = &mut node.source {
            source.content_hash = revised_hash;
        }
    }
    for provenance in &mut snapshot.provenance {
        provenance.producer_version = "2".to_owned();
        provenance.evidence_class = EvidenceClass::SemanticInference;
        if let Some(source) = &mut provenance.source {
            source.content_hash = revised_hash;
        }
    }
    store.apply_delta(GraphDelta {
        repository: repository(),
        worktree: worktree(),
        run_id: IndexRunId::derive(&[b"temporal-context-revision"]),
        expected_base: Some(base),
        next_generation: GenerationId::derive(&[b"temporal-context-revised-generation"]),
        changed_files: snapshot.files,
        removed_files: Vec::new(),
        upsert_provenance: snapshot.provenance,
        upsert_nodes: snapshot.nodes,
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    Ok(())
}

fn ranked_pack(
    store: &dyn GraphStore,
    generation: GenerationId,
    historical: bool,
) -> Result<ContextPack, StoreError> {
    let mut plan = request(true);
    plan.seed_nodes
        .push(NodeId::derive(&[b"context-conformance-helper"]));
    plan.max_hops = 0;
    let query = Query::new(store, generation);
    let source = ContextFixtureSource(SOURCE.to_vec());
    let result = if historical {
        query.historical_ranked_seeded_context(&plan, &source, &ContextFixtureCounter)
    } else {
        query.ranked_seeded_context(&plan, &source, &ContextFixtureCounter)
    };
    result.map_err(|error| StoreError::Backend(error.to_string()))
}

fn verify(
    store: &dyn GraphStore,
    generation: GenerationId,
    expected: &[ContextPack; 7],
) -> Result<(), StoreError> {
    for (seeded, reference) in [false, true].into_iter().zip(expected) {
        let pack = historical_pack(store, generation, seeded)?;
        if &pack != reference
            || pack
                .items
                .iter()
                .any(|item| item.evidence_class != Some(EvidenceClass::SourceFact))
        {
            return Err(StoreError::Integrity(
                "retained context mixed generations or provenance".to_owned(),
            ));
        }
        let serialized =
            serde_json::to_string(&pack).map_err(|error| StoreError::Backend(error.to_string()))?;
        if pack.token_count
            != u64::try_from(serialized.len())
                .map_err(|error| StoreError::Backend(error.to_string()))?
            || pack.token_count > pack.token_budget
        {
            return Err(StoreError::Integrity(
                "temporal context budget is not serialized-exact".to_owned(),
            ));
        }
    }
    if ranked_pack(store, generation, true)? != expected[2] {
        return Err(StoreError::Integrity(
            "retained ranked plan changed after edit or restart".to_owned(),
        ));
    }
    if occurrence_pack(store, generation, true)? != expected[5] {
        return Err(StoreError::Integrity(
            "retained module occurrence packing differs".to_owned(),
        ));
    }
    if packing_plan::pack(store, generation, true)? != expected[6] {
        return Err(StoreError::Integrity(
            "retained forty-node packing plan differs".to_owned(),
        ));
    }
    for (reverse, baseline) in [false, true].into_iter().zip(expected.iter().skip(3)) {
        if saturated_ranked_pack(store, generation, reverse, true)? != *baseline {
            return Err(StoreError::Integrity(
                "retained saturated ranked plan changed".to_owned(),
            ));
        }
    }
    let stale = Query::new(store, generation)
        .historical_context(
            &request(false),
            &ContextFixtureSource(b"changed source".to_vec()),
            &ContextFixtureCounter,
        )
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    if stale
        .items
        .iter()
        .any(|item| item.kind == ContextItemKind::SourceEvidence)
        || !stale
            .warnings
            .iter()
            .any(|warning| warning.code == "stale_source")
    {
        return Err(StoreError::Integrity(
            "temporal context accepted stale source".to_owned(),
        ));
    }
    let latest = GenerationId::derive(&[b"temporal-context-revised-generation"]);
    let current = Query::new(store, latest)
        .historical_context(
            &request(false),
            &ContextFixtureSource(REVISED_SOURCE.to_vec()),
            &ContextFixtureCounter,
        )
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    if current.items.is_empty()
        || !current
            .items
            .iter()
            .any(|item| item.kind == ContextItemKind::SourceEvidence)
        || current
            .items
            .iter()
            .any(|item| item.evidence_class != Some(EvidenceClass::SemanticInference))
        || !current
            .items
            .iter()
            .any(|item| item.text.contains("_later"))
    {
        return Err(StoreError::Integrity(
            "temporal context failed to select revised generation".to_owned(),
        ));
    }
    let mut missing_seed = request(true);
    let missing_id = NodeId::derive(&[b"missing-historical-context-seed"]);
    missing_seed.seed_nodes = vec![missing_id];
    if !matches!(Query::new(store, generation).historical_context(&missing_seed, &ContextFixtureSource(SOURCE.to_vec()), &ContextFixtureCounter), Err(QueryError::UnknownSeed(id)) if id == missing_id)
    {
        return Err(StoreError::Integrity(
            "temporal context accepted a missing seed".to_owned(),
        ));
    }
    let mut empty_request = request(false);
    empty_request.query.clear();
    let empty = Query::new(store, generation)
        .historical_context(
            &empty_request,
            &ContextFixtureSource(SOURCE.to_vec()),
            &ContextFixtureCounter,
        )
        .map_err(|error| StoreError::Backend(error.to_string()))?;
    if !empty.items.is_empty()
        || empty.generation != generation
        || empty.token_count > empty.token_budget
    {
        return Err(StoreError::Integrity(
            "empty temporal context is invalid".to_owned(),
        ));
    }
    let unknown = GenerationId::derive(&[b"unknown-temporal-context"]);
    if !matches!(
        Query::new(store, unknown).historical_context(
            &request(false),
            &ContextFixtureSource(SOURCE.to_vec()),
            &ContextFixtureCounter
        ),
        Err(QueryError::Store(_))
    ) {
        return Err(StoreError::Integrity(
            "temporal context accepted unknown generation".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn temporal_context_matches_current_then_retains_evidence_across_edits_and_restart()
-> Result<(), StoreError> {
    let directory = tempfile::tempdir().map_err(|error| StoreError::Backend(error.to_string()))?;
    let file_path = directory.path().join("graph.snapshot");
    let sqlite_path = directory.path().join("graph.sqlite");
    let turso_path = directory.path().join("graph.turso");
    let mut memory = InMemoryGraphStore::new();
    let base = publish_context_fixture(&mut memory)?;
    let saturation = saturation_delta(base);
    let saturation_generation = saturation.next_generation;
    memory.apply_delta(saturation.clone())?;
    let occurrences = occurrence_delta(&memory, saturation_generation)?;
    let generation = occurrences.next_generation;
    memory.apply_delta(occurrences.clone())?;
    let expected = [
        historical_pack(&memory, generation, false)?,
        historical_pack(&memory, generation, true)?,
        ranked_pack(&memory, generation, true)?,
        saturated_ranked_pack(&memory, generation, false, true)?,
        saturated_ranked_pack(&memory, generation, true, true)?,
        occurrence_pack(&memory, generation, true)?,
        packing_plan::pack(&memory, generation, true)?,
    ];
    if context_pack(&memory, generation)? != expected[0] {
        return Err(StoreError::Integrity(
            "historical/current compiler parity failed".to_owned(),
        ));
    }
    let mut file = FileGraphStore::open(&file_path)?;
    let mut sqlite = open_migrated(&sqlite_path)?;
    let mut turso = open_turso(&turso_path)?;
    for store in [&mut file as &mut dyn GraphStore, &mut sqlite, &mut turso] {
        publish_context_fixture(store)?;
        store.apply_delta(saturation.clone())?;
        store.apply_delta(occurrences.clone())?;
    }
    for store in [
        &mut memory as &mut dyn GraphStore,
        &mut file,
        &mut sqlite,
        &mut turso,
    ] {
        if context_pack(store, generation)? != expected[0] {
            return Err(StoreError::Integrity(
                "current context backend parity failed".to_owned(),
            ));
        }
        if ranked_pack(store, generation, false)? != expected[2] {
            return Err(StoreError::Integrity(
                "ranked current/historical backend parity failed".to_owned(),
            ));
        }
        if occurrence_pack(store, generation, false)? != expected[5] {
            return Err(StoreError::Integrity(
                "current module occurrence packing differs".to_owned(),
            ));
        }
        if packing_plan::pack(store, generation, false)? != expected[6] {
            return Err(StoreError::Integrity(
                "current forty-node packing plan differs".to_owned(),
            ));
        }
        for (reverse, baseline) in [false, true].into_iter().zip(expected.iter().skip(3)) {
            if saturated_ranked_pack(store, generation, reverse, false)? != *baseline {
                return Err(StoreError::Integrity(
                    "current saturated ranked backend parity failed".to_owned(),
                ));
            }
        }
        revise(store, generation)?;
        verify(store, generation, &expected)?;
    }
    drop(file);
    drop(sqlite);
    drop(turso);
    for store in [
        &FileGraphStore::open(&file_path)? as &dyn GraphStore,
        &SqliteGraphStore::open(&sqlite_path)?,
        &open_turso(&turso_path)?,
    ] {
        verify(store, generation, &expected)?;
    }
    Ok(())
}
