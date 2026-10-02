use std::cell::Cell;

use syntaxmesh_core::{
    AcceptanceTime, Edge, EdgeDirection, EdgeId, FileId, FileVersion, GenerationHistoryEntry,
    GenerationId, GenerationManifest, GenerationStatus, GraphDelta, Node, NodeId, NodeKind,
    Provenance, ProvenanceId, RelationKind, RepositoryId, WorktreeId,
};
use syntaxmesh_store::{GraphStore, HistoricalEdgePage, StoreError};

use super::{ContextGraph, ReadMode};
use crate::{Query, QueryError};

struct EdgeOnlyStore {
    generation: GenerationId,
    edges: Vec<Edge>,
    node_reads: Cell<usize>,
    page_reads: Cell<usize>,
    hydrated_nodes: Option<Vec<Node>>,
}

// Any unrelated store operation would make this port-routing fixture fail.
macro_rules! unsupported {
    ($name:ident($($argument:ident: $ty:ty),*) -> $result:ty) => {
        fn $name(&self, $($argument: $ty),*) -> Result<$result, StoreError> {
            $(let _ = $argument;)*
            Err(StoreError::Integrity(stringify!($name).to_owned()))
        }
    };
}

impl GraphStore for EdgeOnlyStore {
    unsupported!(current_generation(repository: RepositoryId, worktree: WorktreeId) -> Option<GenerationManifest>);
    unsupported!(manifest(generation: GenerationId) -> GenerationManifest);
    unsupported!(generation_history() -> Vec<GenerationHistoryEntry>);
    unsupported!(acceptance_time(generation: GenerationId) -> Option<AcceptanceTime>);
    unsupported!(node(generation: GenerationId, id: NodeId) -> Option<Node>);
    unsupported!(nodes(generation: GenerationId) -> Vec<Node>);
    unsupported!(files(generation: GenerationId) -> Vec<FileVersion>);
    unsupported!(edges(generation: GenerationId) -> Vec<Edge>);
    unsupported!(provenance(generation: GenerationId) -> Vec<Provenance>);
    unsupported!(outgoing(generation: GenerationId, id: NodeId) -> Vec<Edge>);
    unsupported!(nodes_for_file(generation: GenerationId, file: FileId) -> Vec<NodeId>);

    fn apply_delta(&mut self, _delta: GraphDelta) -> Result<GenerationManifest, StoreError> {
        Err(StoreError::Integrity("unexpected write".to_owned()))
    }

    fn apply_delta_with_acceptance_time(
        &mut self,
        _delta: GraphDelta,
        _accepted_at: Option<AcceptanceTime>,
    ) -> Result<GenerationManifest, StoreError> {
        Err(StoreError::Integrity("unexpected write".to_owned()))
    }

    fn set_generation_status(
        &mut self,
        _generation: GenerationId,
        _status: GenerationStatus,
    ) -> Result<GenerationManifest, StoreError> {
        Err(StoreError::Integrity("unexpected write".to_owned()))
    }

    fn historical_node(
        &self,
        _generation: GenerationId,
        id: NodeId,
    ) -> Result<Option<Node>, StoreError> {
        self.node_reads.set(self.node_reads.get().saturating_add(1));
        if let Some(nodes) = &self.hydrated_nodes {
            return Ok(nodes.iter().find(|node| node.id == id).cloned());
        }
        Err(StoreError::Integrity(
            "neighbor hydration requested".to_owned(),
        ))
    }

    fn historical_incident_edges(
        &self,
        generation: GenerationId,
        endpoint: NodeId,
        direction: EdgeDirection,
        after: Option<EdgeId>,
        limit: usize,
    ) -> Result<HistoricalEdgePage, StoreError> {
        if generation != self.generation {
            return Err(StoreError::Integrity("wrong fixture generation".to_owned()));
        }
        self.page_reads.set(self.page_reads.get().saturating_add(1));
        let mut items = self
            .edges
            .iter()
            .filter(|edge| {
                after.is_none_or(|id| edge.id > id)
                    && match direction {
                        EdgeDirection::Outgoing => edge.source == endpoint,
                        EdgeDirection::Incoming => edge.target == endpoint,
                    }
            })
            .cloned()
            .collect::<Vec<_>>();
        let has_more = items.len() > limit;
        items.truncate(limit);
        Ok(HistoricalEdgePage::new(items, has_more))
    }
}

fn high_degree_fixture() -> (EdgeOnlyStore, NodeId, NodeId) {
    let generation = GenerationId::derive(&[b"edge-only-context-generation"]);
    let endpoint = NodeId::derive(&[b"edge-only-context-endpoint"]);
    let other = NodeId::derive(&[b"edge-only-context-other"]);
    let mut edges = (0_u32..1001)
        .map(|index| Edge {
            id: EdgeId::derive(&[b"edge-only-context-edge", &index.to_le_bytes()]),
            source: endpoint,
            target: other,
            relation: RelationKind::Calls,
            provenance: ProvenanceId::derive(&[b"edge-only-context-provenance"]),
            extension_payload: None,
        })
        .collect::<Vec<_>>();
    edges.sort_by_key(|edge| edge.id);
    let store = EdgeOnlyStore {
        generation,
        edges,
        node_reads: Cell::new(0),
        page_reads: Cell::new(0),
        hydrated_nodes: None,
    };
    (store, endpoint, other)
}

#[test]
fn temporal_context_pages_edges_without_hydrating_discarded_neighbors() -> Result<(), QueryError> {
    let (store, endpoint, other) = high_degree_fixture();
    let edges = &store.edges;
    let generation = store.generation;
    let query = Query::new(&store, generation);
    let graph = ContextGraph::new(&query, ReadMode::Historical);
    for (id, expected_pages) in [(endpoint, 3), (other, 6)] {
        let mut actual = Vec::new();
        graph.visit_adjacent(id, |edge| {
            actual.push(edge);
            Ok(())
        })?;
        if actual != *edges
            || store.page_reads.get() != expected_pages
            || store.node_reads.get() != 0
        {
            return Err(QueryError::Context(
                "edge-only pagination or hydration differed".to_owned(),
            ));
        }
    }
    if query
        .historical_neighbors(endpoint, EdgeDirection::Outgoing, 1, None)
        .is_ok()
        || store.node_reads.get() != 1
    {
        return Err(QueryError::Context(
            "public neighbor hydration was skipped".to_owned(),
        ));
    }

    let (_page, cursor) = query.historical_edge_page(endpoint, EdgeDirection::Outgoing, 1, None)?;
    let reads = store.page_reads.get();
    if !matches!(
        query.historical_edge_page(other, EdgeDirection::Outgoing, 1, cursor),
        Err(QueryError::InvalidHistoricalNeighborCursor)
    ) || !matches!(
        query.historical_edge_page(endpoint, EdgeDirection::Outgoing, 0, None),
        Err(QueryError::InvalidLimit)
    ) || store.page_reads.get() != reads
    {
        return Err(QueryError::Context(
            "invalid cursor or limit reached the store".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn temporal_adjacency_merge_matches_stable_sort_and_propagates_callback_failure()
-> Result<(), QueryError> {
    let (mut store, endpoint, other) = high_degree_fixture();
    let incoming = (0_u32..1001).map(|index| Edge {
        id: EdgeId::derive(&[b"interleaved-incoming-edge", &index.to_le_bytes()]),
        source: other,
        target: endpoint,
        relation: RelationKind::Calls,
        provenance: ProvenanceId::derive(&[b"interleaved-provenance"]),
        extension_payload: None,
    });
    store.edges.extend(incoming);
    store.edges.push(Edge {
        id: EdgeId::derive(&[b"interleaved-self-edge"]),
        source: endpoint,
        target: endpoint,
        relation: RelationKind::Calls,
        provenance: ProvenanceId::derive(&[b"interleaved-provenance"]),
        extension_payload: None,
    });
    store.edges.sort_by_key(|edge| edge.id);
    let mut expected = store
        .edges
        .iter()
        .filter(|edge| edge.source == endpoint)
        .cloned()
        .collect::<Vec<_>>();
    expected.extend(
        store
            .edges
            .iter()
            .filter(|edge| edge.target == endpoint)
            .cloned(),
    );
    expected.sort_by_key(|edge| edge.id);
    let query = Query::new(&store, store.generation);
    let graph = ContextGraph::new(&query, ReadMode::Historical);
    let mut actual = Vec::new();
    graph.visit_adjacent(endpoint, |edge| {
        if actual.is_empty() && store.page_reads.get() != 2 {
            return Err(QueryError::Context(
                "adjacency was eagerly materialized".to_owned(),
            ));
        }
        actual.push(edge);
        Ok(())
    })?;
    if actual != expected || store.page_reads.get() != 4 || store.node_reads.get() != 0 {
        return Err(QueryError::Context(
            "streamed adjacency differs from stable-sort oracle".to_owned(),
        ));
    }
    store.page_reads.set(0);
    let failure = graph.visit_adjacent(endpoint, |_edge| {
        Err(QueryError::Context("callback stopped".to_owned()))
    });
    if !matches!(failure, Err(QueryError::Context(message)) if message == "callback stopped")
        || store.page_reads.get() != 2
    {
        return Err(QueryError::Context(
            "callback failure fetched later pages or was swallowed".to_owned(),
        ));
    }
    let mut empty_count = 0_usize;
    graph.visit_adjacent(NodeId::derive(&[b"empty-adjacency"]), |_edge| {
        empty_count = empty_count.saturating_add(1);
        Ok(())
    })?;
    if empty_count != 0 {
        return Err(QueryError::Context(
            "empty endpoint returned edges".to_owned(),
        ));
    }
    Ok(())
}

#[test]
fn saturated_context_retains_only_edges_between_selected_endpoints() -> Result<(), QueryError> {
    let (mut store, endpoint, other) = high_degree_fixture();
    let mut self_edge = store
        .edges
        .first()
        .cloned()
        .ok_or_else(|| QueryError::Context("high-degree fixture has no edges".to_owned()))?;
    self_edge.id = EdgeId::derive(&[b"selected-context-self-edge"]);
    self_edge.target = endpoint;
    let mut incoming = self_edge.clone();
    incoming.id = EdgeId::derive(&[b"selected-context-incoming-edge"]);
    incoming.source = other;
    store.edges.extend([self_edge.clone(), incoming]);
    store.edges.sort_by_key(|edge| edge.id);
    let node = |id| Node {
        id,
        kind: NodeKind::Function,
        name: id.0.to_hex(),
        owner_file: None,
        source: None,
        provenance: self_edge.provenance,
        extension_payload: None,
    };
    store.hydrated_nodes = Some(vec![node(endpoint), node(other)]);
    for (capacity, preselected) in [(1, false), (2, false), (2, true)] {
        store.node_reads.set(0);
        let query = Query::new(&store, store.generation);
        let graph = ContextGraph::new(&query, ReadMode::Historical);
        let mut nodes = std::collections::BTreeMap::from([(endpoint, (node(endpoint), 0, 0))]);
        if preselected {
            nodes.insert(other, (node(other), 0, 0));
        }
        let mut retained = std::collections::BTreeMap::new();
        super::super::expand_neighborhood(
            &graph,
            &mut nodes,
            &mut retained,
            2,
            capacity,
            super::super::SeedMode::ExplicitOnly,
        )?;
        let expected = store
            .edges
            .iter()
            .filter(|edge| nodes.contains_key(&edge.source) && nodes.contains_key(&edge.target))
            .map(|edge| (edge.id, edge.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let expected_reads = usize::from(capacity == 2 && !preselected);
        if retained != expected
            || nodes.len() != capacity
            || store.node_reads.get() != expected_reads
        {
            return Err(QueryError::Context(
                "selected context edge retention differs".to_owned(),
            ));
        }
        if capacity == 1 && retained.len() != 1 {
            return Err(QueryError::Context(
                "discarded endpoints consumed edge retention".to_owned(),
            ));
        }
    }
    Ok(())
}
#[test]
fn ranked_frontier_admits_priority_neighbor_before_canonical_seed() -> Result<(), QueryError> {
    let (mut store, first, second) = high_degree_fixture();
    let (low_id, high_id) = if first < second {
        (first, second)
    } else {
        (second, first)
    };
    let low_neighbor = NodeId::derive(&[b"low-priority-neighbor"]);
    let high_neighbor = NodeId::derive(&[b"high-priority-neighbor"]);
    let provenance = ProvenanceId::derive(&[b"ranked-frontier-fixture"]);
    let node = |id| Node {
        id,
        kind: NodeKind::Function,
        name: id.0.to_hex(),
        owner_file: None,
        source: None,
        provenance,
        extension_payload: None,
    };
    store.edges = [(low_id, low_neighbor), (high_id, high_neighbor)]
        .into_iter()
        .map(|(source, target)| Edge {
            id: EdgeId::derive(&[&source.0.0, &target.0.0]),
            source,
            target,
            relation: RelationKind::Calls,
            provenance,
            extension_payload: None,
        })
        .collect();
    store.edges.sort_by_key(|edge| edge.id);
    store.hydrated_nodes = Some(
        [low_id, high_id, low_neighbor, high_neighbor]
            .into_iter()
            .map(node)
            .collect(),
    );
    for (mode, admitted, rejected) in [
        (
            super::super::SeedMode::ExplicitOnly,
            low_neighbor,
            high_neighbor,
        ),
        (
            super::super::SeedMode::RankedExplicit,
            high_neighbor,
            low_neighbor,
        ),
    ] {
        store.node_reads.set(0);
        let query = Query::new(&store, store.generation);
        let graph = ContextGraph::new(&query, ReadMode::Historical);
        let mut nodes = std::collections::BTreeMap::from([
            (low_id, (node(low_id), 0, u32::MAX - 1)),
            (high_id, (node(high_id), 0, u32::MAX)),
        ]);
        let mut edges = std::collections::BTreeMap::new();
        super::super::expand_neighborhood(&graph, &mut nodes, &mut edges, 2, 3, mode)?;
        if !nodes.contains_key(&admitted)
            || nodes.contains_key(&rejected)
            || nodes
                .get(&admitted)
                .is_none_or(|entry| entry.1 != 1 || entry.2 != 0)
            || store.node_reads.get() != 1
            || edges.len() != 1
        {
            return Err(QueryError::Context(
                "ranked frontier admission or hydration changed".to_owned(),
            ));
        }
    }
    Ok(())
}
