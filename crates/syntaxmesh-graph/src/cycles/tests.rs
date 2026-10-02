use super::*;
use syntaxmesh_core::{
    Edge, EdgeId, EvidenceClass, GenerationId, GraphDelta, IndexRunId, Node, NodeKind, Provenance,
    ProvenanceId, RepositoryId, WorktreeId,
};
use syntaxmesh_store::{GraphStore as _, InMemoryGraphStore};

fn fixture(edges: &[(usize, usize, RelationKind)]) -> Result<GenerationGraph, StoreError> {
    let (store, generation) = fixture_store(edges)?;
    GenerationGraph::load(&store, generation)
}

fn fixture_store(
    edges: &[(usize, usize, RelationKind)],
) -> Result<(InMemoryGraphStore, GenerationId), StoreError> {
    let provenance = Provenance {
        id: ProvenanceId::derive(&[b"cycles"]),
        producer_namespace: "test.cycles".to_owned(),
        producer_version: "1".to_owned(),
        evidence_class: EvidenceClass::UserAsserted,
        source: None,
    };
    let nodes = (0usize..6)
        .map(|index| Node {
            id: NodeId::derive(&[index.to_string().as_bytes()]),
            kind: NodeKind::Function,
            name: index.to_string(),
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        })
        .collect::<Vec<_>>();
    let edges = edges
        .iter()
        .enumerate()
        .map(|(index, (source, target, relation))| {
            Ok(Edge {
                id: EdgeId::derive(&[index.to_string().as_bytes()]),
                source: nodes
                    .get(*source)
                    .ok_or_else(|| StoreError::Integrity("bad fixture source".to_owned()))?
                    .id,
                target: nodes
                    .get(*target)
                    .ok_or_else(|| StoreError::Integrity("bad fixture target".to_owned()))?
                    .id,
                relation: relation.clone(),
                provenance: provenance.id,
                extension_payload: None,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    let generation = GenerationId::derive(&[b"cycles"]);
    let mut store = InMemoryGraphStore::new();
    store.apply_delta(GraphDelta {
        repository: RepositoryId::derive(&[b"cycles"]),
        worktree: WorktreeId::derive(&[b"cycles"]),
        run_id: IndexRunId::derive(&[b"cycles"]),
        expected_base: None,
        next_generation: generation,
        changed_files: vec![],
        removed_files: vec![],
        upsert_provenance: vec![provenance],
        upsert_nodes: nodes,
        remove_nodes: vec![],
        upsert_edges: edges,
        remove_edges: vec![],
    })?;
    Ok((store, generation))
}

#[test]
fn retained_cycles_survive_later_edge_removal() -> Result<(), StoreError> {
    let (mut store, old) =
        fixture_store(&[(0, 1, RelationKind::Calls), (1, 0, RelationKind::Calls)])?;
    let original = GenerationGraph::load_retained(&store, old)?.cyclic_components(None)?;
    let current = GenerationId::derive(&[b"cycles-after-removal"]);
    store.apply_delta(GraphDelta {
        repository: RepositoryId::derive(&[b"cycles"]),
        worktree: WorktreeId::derive(&[b"cycles"]),
        run_id: IndexRunId::derive(&[b"cycles-after-removal"]),
        expected_base: Some(old),
        next_generation: current,
        changed_files: vec![],
        removed_files: vec![],
        upsert_provenance: vec![],
        upsert_nodes: vec![],
        remove_nodes: vec![],
        upsert_edges: vec![],
        remove_edges: vec![EdgeId::derive(&[b"0"])],
    })?;
    if original.len() != 1
        || GenerationGraph::load_retained(&store, old)?.cyclic_components(None)? != original
        || !GenerationGraph::load_retained(&store, current)?
            .cyclic_components(None)?
            .is_empty()
        || GenerationGraph::load_retained(&store, GenerationId::derive(&[b"cycles-missing"]))
            .is_ok()
    {
        return Err(StoreError::Integrity(
            "retained cycle projection changed after publication".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn directed_components_filter_relations_self_loops_and_acyclic_nodes() -> Result<(), StoreError> {
    let graph = fixture(&[
        (0, 1, RelationKind::Calls),
        (1, 0, RelationKind::Calls),
        (1, 2, RelationKind::Calls),
        (2, 3, RelationKind::Imports),
        (3, 2, RelationKind::Calls),
        (4, 4, RelationKind::Calls),
    ])?;
    let node = |index: usize| NodeId::derive(&[index.to_string().as_bytes()]);
    let sorted = |mut components: Vec<Vec<NodeId>>| {
        for component in &mut components {
            component.sort_unstable();
        }
        components.sort_unstable();
        components
    };
    if graph.cyclic_components(None)?
        != sorted(vec![
            vec![node(0), node(1)],
            vec![node(2), node(3)],
            vec![node(4)],
        ])
        || graph.cyclic_components(Some(&RelationKind::Calls))?
            != sorted(vec![vec![node(0), node(1)], vec![node(4)]])
        || !graph
            .cyclic_components(Some(&RelationKind::Defines))?
            .is_empty()
        || graph.cyclic_components(None)? != graph.cyclic_components(None)?
    {
        return Err(StoreError::Integrity(
            "cycle component semantics differ".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn selected_dangling_edges_fail_closed() -> Result<(), StoreError> {
    let mut graph = fixture(&[(0, 1, RelationKind::Calls)])?;
    graph.nodes.remove(&NodeId::derive(&[b"1"]));
    if graph.cyclic_components(None).is_ok()
        || !graph
            .cyclic_components(Some(&RelationKind::Imports))?
            .is_empty()
    {
        return Err(StoreError::Integrity(
            "dangling cycle edge accepted".to_owned(),
        ));
    }
    Ok(())
}
