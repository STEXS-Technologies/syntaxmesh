use super::*;
use syntaxmesh_api_model::{TEMPORAL_EXPORT_SCHEMA_VERSION, TemporalRecord};
use syntaxmesh_core::{FactChangeKind, FactVersionRef, Node, NodeKind};

#[test]
fn exact_fact_version_introduction_tracks_add_update_and_reintroduction() -> Result<(), QueryError>
{
    let generations = [
        GenerationId::derive(&[b"introduction-added"]),
        GenerationId::derive(&[b"introduction-updated"]),
        GenerationId::derive(&[b"introduction-removed"]),
        GenerationId::derive(&[b"introduction-readded"]),
        GenerationId::derive(&[b"introduction-noop"]),
    ];
    let [added, updated, removed, readded, noop] = generations;
    let repository = RepositoryId::derive(&[b"introduction-repository"]);
    let worktree = WorktreeId::derive(&[b"introduction-worktree"]);
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"introduction-provenance"]),
        producer_namespace: "test.fact-introduction".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let node = Node {
        id: NodeId::derive(&[b"introduction-node"]),
        kind: NodeKind::Function,
        name: "before".to_owned(),
        owner_file: None,
        source: None,
        provenance: provenance.id,
        extension_payload: None,
    };
    let mut store = InMemoryGraphStore::new();
    let apply = |target_store: &mut InMemoryGraphStore,
                 expected_base,
                 next_generation,
                 run: &[u8],
                 upsert_nodes,
                 remove_nodes| {
        target_store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[run]),
            expected_base,
            next_generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: Vec::new(),
            upsert_nodes,
            upsert_edges: Vec::new(),
            remove_nodes,
            remove_edges: Vec::new(),
        })
    };
    store.apply_delta(GraphDelta {
        repository,
        worktree,
        run_id: IndexRunId::derive(&[b"introduction-first-run"]),
        expected_base: None,
        next_generation: added,
        changed_files: Vec::new(),
        removed_files: Vec::new(),
        upsert_provenance: vec![provenance],
        upsert_nodes: vec![node.clone()],
        upsert_edges: Vec::new(),
        remove_nodes: Vec::new(),
        remove_edges: Vec::new(),
    })?;
    let mut changed_node = node.clone();
    changed_node.name = "after".to_owned();
    apply(
        &mut store,
        Some(added),
        updated,
        b"introduction-update",
        vec![changed_node.clone()],
        Vec::new(),
    )?;
    apply(
        &mut store,
        Some(updated),
        removed,
        b"introduction-remove",
        Vec::new(),
        vec![node.id],
    )?;
    apply(
        &mut store,
        Some(removed),
        readded,
        b"introduction-readd",
        vec![changed_node],
        Vec::new(),
    )?;
    apply(
        &mut store,
        Some(readded),
        noop,
        b"introduction-noop",
        Vec::new(),
        Vec::new(),
    )?;

    let query = Query::new(&store, noop);
    let fact = FactRef::Node(node.id);
    let assert_event = |generation, expected_kind| -> Result<(), QueryError> {
        let version = FactVersionRef {
            fact,
            valid_from: generation,
        };
        let event = store
            .change_event(generation)?
            .ok_or(QueryError::InvalidLimit)?;
        if !matches!(
            query.fact_version_introduction(version)?,
            Some(TemporalRecord::FactVersionIntroduction {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                version: selected,
                event: selected_event,
            }) if selected == version
                && selected_event == event
                && event.generation_after == generation
                && event.changed_facts.iter().any(|changed| {
                    changed.fact == fact && changed.kind == expected_kind
                })
        ) {
            return Err(QueryError::InvalidLimit);
        }
        Ok(())
    };
    assert_event(added, FactChangeKind::Added)?;
    assert_event(updated, FactChangeKind::Updated)?;
    assert_event(readded, FactChangeKind::Added)?;
    for (introduced_at, retired_at, expected_kind) in [
        (added, updated, FactChangeKind::Updated),
        (updated, removed, FactChangeKind::Removed),
    ] {
        let version = FactVersionRef {
            fact,
            valid_from: introduced_at,
        };
        let expected_event = store
            .change_event(retired_at)?
            .ok_or(QueryError::InvalidLimit)?;
        if !matches!(
            query.fact_version_retirement(version)?,
            Some(TemporalRecord::FactVersionRetirement {
                schema_version: TEMPORAL_EXPORT_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                version: selected,
                event,
            }) if selected == version
                && event == expected_event
                && event.changed_facts.iter().any(|changed| {
                    changed.fact == fact && changed.kind == expected_kind
                })
        ) {
            return Err(QueryError::InvalidLimit);
        }
    }
    if query
        .fact_version_retirement(FactVersionRef {
            fact,
            valid_from: readded,
        })?
        .is_some()
        || Query::new(&store, added)
            .fact_version_retirement(FactVersionRef {
                fact,
                valid_from: added,
            })?
            .is_some()
    {
        return Err(QueryError::InvalidLimit);
    }
    if query
        .fact_version_introduction(FactVersionRef {
            fact,
            valid_from: removed,
        })?
        .is_some()
        || query
            .fact_version_introduction(FactVersionRef {
                fact,
                valid_from: noop,
            })?
            .is_some()
    {
        return Err(QueryError::InvalidLimit);
    }
    if Query::new(&store, added)
        .fact_version_introduction(FactVersionRef {
            fact,
            valid_from: updated,
        })?
        .is_some()
    {
        return Err(QueryError::InvalidLimit);
    }
    let missing = GenerationId::derive(&[b"introduction-missing"]);
    if query
        .fact_version_introduction(FactVersionRef {
            fact,
            valid_from: missing,
        })
        .is_ok()
    {
        return Err(QueryError::InvalidLimit);
    }
    Ok(())
}
