//! Read routing only; selection and evidence packing remain in the shared compiler.

use syntaxmesh_core::{Edge, Node, NodeId, Provenance, ProvenanceId};
use syntaxmesh_store::GraphStore;

use crate::{Query, QueryError};

mod adjacency;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
pub(super) enum ReadMode {
    Current,
    Historical,
}

pub(super) struct ContextGraph<'query, 'store, S: ?Sized> {
    query: &'query Query<'store, S>,
    mode: ReadMode,
}

impl<'query, 'store, S: GraphStore + ?Sized> ContextGraph<'query, 'store, S> {
    pub(super) const fn new(query: &'query Query<'store, S>, mode: ReadMode) -> Self {
        Self { query, mode }
    }

    pub(super) fn node(&self, id: NodeId) -> Result<Option<Node>, QueryError> {
        match self.mode {
            ReadMode::Current => self.query.node(id),
            ReadMode::Historical => self.query.historical_node(id),
        }
    }

    pub(super) fn search(&self, text: &str, limit: usize) -> Result<Vec<Node>, QueryError> {
        match self.mode {
            ReadMode::Current => self.query.search(text, limit),
            ReadMode::Historical => self.query.historical_search(text, limit),
        }
    }

    pub(super) fn visit_adjacent(
        &self,
        id: NodeId,
        mut visitor: impl FnMut(Edge) -> Result<(), QueryError>,
    ) -> Result<(), QueryError> {
        if matches!(self.mode, ReadMode::Current) {
            let mut adjacent = self.query.neighbor_edges(id)?;
            adjacent.extend(self.query.incoming_edges(id)?);
            adjacent.sort_by_key(|edge| edge.id);
            for edge in adjacent {
                visitor(edge)?;
            }
            return Ok(());
        }
        adjacency::visit(self.query, id, visitor)
    }

    pub(super) fn context_provenance(
        &self,
        ids: &[ProvenanceId],
    ) -> Result<Vec<Provenance>, QueryError> {
        if matches!(self.mode, ReadMode::Current) {
            return self.query.context_provenance(ids);
        }
        ids.iter()
            .map(|id| self.query.historical_provenance(*id))
            .collect::<Result<Vec<_>, _>>()
            .map(|records| records.into_iter().flatten().collect())
    }
}
