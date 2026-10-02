use petgraph::algo::kosaraju_scc;
use petgraph::graphmap::DiGraphMap;
use syntaxmesh_core::{NodeId, RelationKind};
use syntaxmesh_store::StoreError;

use crate::GenerationGraph;

#[cfg(test)]
mod tests;

impl GenerationGraph {
    /// Return deterministic cyclic strongly connected components.
    /// An exact relation filter excludes other relationships before analysis.
    /// Singleton components are returned only if they have a selected self-loop.
    /// The SCC kernel is O(V+E); endpoint checks and deterministic sorting add
    /// overhead. This does not enumerate individual cycle paths.
    ///
    /// # Errors
    /// Rejects selected edges whose endpoints are absent from the projection.
    pub fn cyclic_components(
        &self,
        relation: Option<&RelationKind>,
    ) -> Result<Vec<Vec<NodeId>>, StoreError> {
        let mut graph = DiGraphMap::<NodeId, ()>::new();
        for id in self.nodes.keys() {
            graph.add_node(*id);
        }
        for edge in self.outgoing.values().flatten() {
            if relation.is_some_and(|selected| selected != &edge.relation) {
                continue;
            }
            if !self.nodes.contains_key(&edge.source) || !self.nodes.contains_key(&edge.target) {
                return Err(StoreError::Integrity(
                    "cycle projection has a dangling endpoint".to_owned(),
                ));
            }
            graph.add_edge(edge.source, edge.target, ());
        }
        let mut components = kosaraju_scc(&graph)
            .into_iter()
            .filter(|members| {
                members.len() > 1
                    || members
                        .first()
                        .is_some_and(|id| graph.contains_edge(*id, *id))
            })
            .map(|mut members| {
                members.sort_unstable();
                members
            })
            .collect::<Vec<_>>();
        components.sort_unstable();
        Ok(components)
    }
}
