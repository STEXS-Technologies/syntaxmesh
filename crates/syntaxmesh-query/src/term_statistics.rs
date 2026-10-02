use crate::{QueryError, identifier_terms};
use std::collections::BTreeMap;
use syntaxmesh_core::GenerationId;
use syntaxmesh_store::{GraphStore, StoreError};

#[cfg(test)]
mod tests;

/// Counts are exact only when `complete` is true; otherwise they are lower bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationTermStatistics {
    pub generation: GenerationId,
    pub scanned_nodes: usize,
    pub frequencies: BTreeMap<String, usize>,
    pub complete: bool,
}

/// Collect one pinned generation's identifier-term document frequencies.
/// Processes at most `scan_budget` nodes in pages of at most 1,000 nodes.
/// Store implementations can impose additional internal costs.
///
/// # Errors
/// Rejects zero budgets, more than 16 normalized terms, and corrupt/store pages.
pub fn generation_term_statistics<S: GraphStore + ?Sized>(
    store: &S,
    generation: GenerationId,
    query: &str,
    scan_budget: usize,
) -> Result<GenerationTermStatistics, QueryError> {
    let terms = identifier_terms(query);
    if scan_budget == 0 || terms.len() > 16 {
        return Err(QueryError::InvalidLimit);
    }
    let mut result = GenerationTermStatistics {
        generation,
        scanned_nodes: 0,
        frequencies: terms.into_iter().map(|term| (term, 0)).collect(),
        complete: false,
    };
    let mut after = None;
    while result.scanned_nodes < scan_budget {
        let limit = scan_budget.saturating_sub(result.scanned_nodes).min(1000);
        let page = store.historical_nodes_page(generation, after, limit)?;
        if page.items.len() > limit || (page.has_more && page.items.is_empty()) {
            return Err(StoreError::Integrity("invalid term-statistics page".to_owned()).into());
        }
        for node in page.items {
            if after.is_some_and(|previous| node.id <= previous) {
                return Err(
                    StoreError::Integrity("nonadvancing term-statistics page".to_owned()).into(),
                );
            }
            after = Some(node.id);
            result.scanned_nodes = result.scanned_nodes.saturating_add(1);
            let normalized = identifier_terms(&node.name);
            for (term, frequency) in &mut result.frequencies {
                if normalized.contains(term) {
                    *frequency = frequency.saturating_add(1);
                }
            }
        }
        if !page.has_more {
            result.complete = true;
            break;
        }
    }
    Ok(result)
}
