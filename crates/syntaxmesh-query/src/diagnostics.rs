use syntaxmesh_core::{GenerationId, Node, NodeId, NodeKind};
use syntaxmesh_store::{HistoricalNodePage, StoreError};

use crate::QueryError;

#[cfg(test)]
mod tests;

/// Diagnostic matches within one bounded, generation-scoped node scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionDiagnosticPage {
    pub generation: GenerationId,
    pub items: Vec<Node>,
    pub scanned_nodes: usize,
    /// Last scanned node when more remain, even if this page contains no matches.
    pub next_after: Option<NodeId>,
}

pub(crate) fn diagnostic_page(
    generation: GenerationId,
    page: HistoricalNodePage,
) -> Result<ResolutionDiagnosticPage, QueryError> {
    let next_after = if page.has_more {
        Some(
            page.items
                .last()
                .ok_or_else(|| {
                    StoreError::Integrity(
                        "historical node page reports continuation without nodes".to_owned(),
                    )
                })?
                .id,
        )
    } else {
        None
    };
    Ok(ResolutionDiagnosticPage {
        generation,
        scanned_nodes: page.items.len(),
        next_after,
        items: page
            .items
            .into_iter()
            .filter(|node| matches!(node.kind, NodeKind::ModuleResolutionDiagnostic { .. }))
            .collect(),
    })
}
