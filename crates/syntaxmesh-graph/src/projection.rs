//! Generation-tagged adjacency projection over canonical graph facts.

use std::collections::BTreeMap;

use syntaxmesh_core::{Edge, GenerationId, Node, NodeId};
use syntaxmesh_store::{GraphStore, StoreError};

/// A read-only adjacency projection for exactly one published generation.
#[derive(Debug, Clone)]
pub struct GenerationGraph {
    generation: GenerationId,
    pub(super) nodes: BTreeMap<NodeId, Node>,
    pub(super) outgoing: BTreeMap<NodeId, Vec<Edge>>,
    incoming: BTreeMap<NodeId, Vec<Edge>>,
}

impl GenerationGraph {
    /// Load a projection from one consistent store generation.
    ///
    /// # Errors
    /// Returns a store error if the generation is stale or unavailable.
    pub fn load<S>(store: &S, generation: GenerationId) -> Result<Self, StoreError>
    where
        S: GraphStore,
    {
        Ok(Self::from_parts(
            generation,
            store.nodes(generation)?,
            store.edges(generation)?,
        ))
    }

    /// Load a complete projection from one retained generation.
    /// Reuses the store's historical snapshot rather than current-only reads.
    /// This allocates the complete selected graph, not a bounded neighborhood.
    ///
    /// # Errors
    /// Returns an error for unavailable or corrupt retained history.
    pub fn load_retained<S: GraphStore + ?Sized>(
        store: &S,
        generation: GenerationId,
    ) -> Result<Self, StoreError> {
        let snapshot = store.historical_snapshot(generation)?;
        Ok(Self::from_parts(generation, snapshot.nodes, snapshot.edges))
    }

    fn from_parts(generation: GenerationId, nodes: Vec<Node>, edges: Vec<Edge>) -> Self {
        let nodes = nodes
            .into_iter()
            .map(|node| (node.id, node))
            .collect::<BTreeMap<_, _>>();
        let mut outgoing = BTreeMap::<NodeId, Vec<Edge>>::new();
        let mut incoming = BTreeMap::<NodeId, Vec<Edge>>::new();
        for edge in edges {
            outgoing.entry(edge.source).or_default().push(edge.clone());
            incoming.entry(edge.target).or_default().push(edge);
        }
        Self {
            generation,
            nodes,
            outgoing,
            incoming,
        }
    }

    /// The exact generation represented by this projection.
    #[must_use]
    pub const fn generation(&self) -> GenerationId {
        self.generation
    }

    /// Look up a node in this generation.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Return outgoing edges for a node.
    #[must_use]
    pub fn outgoing(&self, id: NodeId) -> &[Edge] {
        self.outgoing.get(&id).map_or(&[], Vec::as_slice)
    }

    /// Return incoming edges for a node.
    #[must_use]
    pub fn incoming(&self, id: NodeId) -> &[Edge] {
        self.incoming.get(&id).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntaxmesh_core::{
        EdgeId, EvidenceClass, GenerationId, GraphDelta, IndexRunId, NodeKind, Provenance,
        ProvenanceId, RelationKind, RepositoryId, WorktreeId,
    };
    use syntaxmesh_store::InMemoryGraphStore;

    #[test]
    fn projection_is_bound_to_one_generation() -> Result<(), StoreError> {
        let repository = RepositoryId::derive(&[b"repo"]);
        let worktree = WorktreeId::derive(&[b"worktree"]);
        let generation = GenerationId::derive(&[b"generation"]);
        let provenance = Provenance {
            id: ProvenanceId::derive(&[b"provenance"]),
            producer_namespace: String::from("test"),
            producer_version: String::from("1"),
            evidence_class: EvidenceClass::SourceFact,
            source: None,
        };
        let first = Node {
            id: NodeId::derive(&[b"first"]),
            kind: NodeKind::Function,
            name: String::from("first"),
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        };
        let second = Node {
            id: NodeId::derive(&[b"second"]),
            kind: NodeKind::Function,
            name: String::from("second"),
            owner_file: None,
            source: None,
            provenance: provenance.id,
            extension_payload: None,
        };
        let edge = Edge {
            id: EdgeId::derive(&[b"edge"]),
            source: first.id,
            target: second.id,
            relation: RelationKind::Calls,
            provenance: provenance.id,
            extension_payload: None,
        };
        let mut store = InMemoryGraphStore::new();
        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"run"]),
            expected_base: None,
            next_generation: generation,
            changed_files: Vec::new(),
            removed_files: Vec::new(),
            upsert_provenance: vec![provenance],
            upsert_nodes: vec![first.clone(), second],
            upsert_edges: vec![edge],
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;
        let graph = GenerationGraph::load(&store, generation)?;
        if graph.generation() != generation
            || graph.node(first.id).is_none()
            || graph.outgoing(first.id).len() != 1
            || !graph.incoming(first.id).is_empty()
        {
            return Err(StoreError::Integrity(String::from(
                "generation projection has incorrect adjacency",
            )));
        }
        Ok(())
    }
}
