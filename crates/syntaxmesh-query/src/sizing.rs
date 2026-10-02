use syntaxmesh_api_model::{
    HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION, TemporalQueryMode, TemporalRecord,
};
use syntaxmesh_core::{Edge, GenerationId, Node};

use crate::QueryError;
use crate::service::serialized_line_size;

/// Independent work and output bounds for an exact-generation neighborhood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoricalNeighborhoodLimits {
    pub max_hops: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_scanned_edges: usize,
    pub max_result_bytes: usize,
}

/// Accounts a caller's output representation without coupling traversal to a host.
/// Sizes must conservatively cover metadata and all selected items/separators.
pub trait HistoricalNeighborhoodOutputSizer {
    /// Reserve metadata bytes, returning `None` if the representation cannot fit.
    ///
    /// # Errors
    /// Returns serialization/accounting errors.
    fn metadata(&self, generation: GenerationId, limit: usize)
    -> Result<Option<usize>, QueryError>;
    /// Account one node and its output separators within the remaining budget.
    ///
    /// # Errors
    /// Returns serialization/accounting errors.
    fn node(
        &self,
        generation: GenerationId,
        depth: usize,
        node: Node,
        remaining: usize,
    ) -> Result<Option<usize>, QueryError>;
    /// Account one edge and its output separators within the remaining budget.
    ///
    /// # Errors
    /// Returns serialization/accounting errors.
    fn edge(
        &self,
        generation: GenerationId,
        depth: usize,
        edge: Edge,
        remaining: usize,
    ) -> Result<Option<usize>, QueryError>;
}

pub(crate) struct ExportOutputSizer;

impl HistoricalNeighborhoodOutputSizer for ExportOutputSizer {
    fn metadata(
        &self,
        generation: GenerationId,
        limit: usize,
    ) -> Result<Option<usize>, QueryError> {
        serialized_line_size(
            &TemporalRecord::HistoricalNeighborhoodFooter {
                schema_version: HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                nodes: u64::MAX,
                edges: u64::MAX,
                scanned_incidence_entries: u64::MAX,
                serialized_item_bytes: u64::MAX,
                truncated: true,
            },
            limit,
        )
    }

    fn node(
        &self,
        generation: GenerationId,
        depth: usize,
        node: Node,
        remaining: usize,
    ) -> Result<Option<usize>, QueryError> {
        serialized_line_size(
            &TemporalRecord::HistoricalNeighborhoodNode {
                schema_version: HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                depth,
                node,
            },
            remaining,
        )
    }

    fn edge(
        &self,
        generation: GenerationId,
        depth: usize,
        edge: Edge,
        remaining: usize,
    ) -> Result<Option<usize>, QueryError> {
        serialized_line_size(
            &TemporalRecord::HistoricalNeighborhoodEdge {
                schema_version: HISTORICAL_NEIGHBORHOOD_SCHEMA_VERSION,
                query_mode: TemporalQueryMode::HistoricalConclusion,
                generation,
                depth,
                edge,
            },
            remaining,
        )
    }
}
